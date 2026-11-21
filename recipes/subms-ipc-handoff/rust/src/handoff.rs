//! Ping-pong drivers, one per transport. Each runs `warmup + n` round trips
//! of one 64-byte message with the two ends pinned to different cores where
//! the platform allows, and returns `n` samples of round trip / 2 in
//! nanoseconds.
//!
//! Round trip / 2 because the two ends of a cross-process handoff do not
//! share a clock you can trust to tens of nanoseconds, and the initiator's
//! own clock reading both edges does. It assumes the legs are symmetric,
//! which holds here: every stage sends the same bytes the same way in both
//! directions, except SSE, whose return leg is a bare 64-byte ack.

use std::io::{self, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Instant;

use crate::pin::{PinGuard, peer_cpus};
use crate::ring::{Endpoint, MSG_LEN, Msg};
use crate::sse::{REQUEST_HEAD, RESPONSE_HEAD, SseReader, SseWriter, skip_head};

/// How a ring receiver waits for the next message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Wait {
    /// Burn the core polling the ring.
    Spin,
    /// Sleep on a futex; the sender pays a syscall to wake it.
    Park,
}

/// The message for round trip `i`: printable ASCII so the SSE stage can carry
/// the identical bytes, sequence number first so the echo can be checked.
pub fn message(i: u64) -> Msg {
    let mut m = [b'.'; MSG_LEN];
    let hex = format!("{i:016x}");
    m[..16].copy_from_slice(hex.as_bytes());
    m
}

fn keep(i: usize, warmup: usize, t0: Instant, out: &mut Vec<u64>) {
    if i >= warmup {
        out.push(t0.elapsed().as_nanos() as u64 / 2);
    }
}

fn join_peer(h: thread::JoinHandle<io::Result<()>>) -> io::Result<()> {
    h.join()
        .map_err(|_| io::Error::other("echo thread panicked"))?
}

// ---------------------------------------------------------------- rings

fn ring_initiate(a: &mut Endpoint, n: usize, warmup: usize, wait: Wait) -> Vec<u64> {
    let mut out = Vec::with_capacity(n);
    let mut back = [0u8; MSG_LEN];
    for i in 0..warmup + n {
        let m = message(i as u64);
        let t0 = Instant::now();
        ring_send(a, &m, wait);
        ring_recv(a, &mut back, wait);
        keep(i, warmup, t0, &mut out);
        debug_assert_eq!(back, m);
    }
    out
}

// Runs inside a forked child on the process stages, so it must not allocate.
fn ring_echo(b: &mut Endpoint, total: usize, wait: Wait) {
    let mut m = [0u8; MSG_LEN];
    for _ in 0..total {
        ring_recv(b, &mut m, wait);
        ring_send(b, &m, wait);
    }
}

fn ring_send(e: &mut Endpoint, m: &Msg, wait: Wait) {
    match wait {
        Wait::Spin => {
            while !e.try_send(m) {
                std::hint::spin_loop();
            }
        }
        Wait::Park => park_send(e, m),
    }
}

fn ring_recv(e: &mut Endpoint, out: &mut Msg, wait: Wait) {
    match wait {
        Wait::Spin => e.recv_spin(out),
        Wait::Park => park_recv(e, out),
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn park_send(e: &mut Endpoint, m: &Msg) {
    while !e.send_wake(m) {
        std::hint::spin_loop();
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn park_recv(e: &mut Endpoint, out: &mut Msg) {
    e.recv_park(out)
}

// Without futexes, Park falls back to a yield loop. Nothing measured uses it;
// it exists so `Wait` means the same thing on every platform.
#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn park_send(e: &mut Endpoint, m: &Msg) {
    while !e.try_send(m) {
        thread::yield_now();
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn park_recv(e: &mut Endpoint, out: &mut Msg) {
    while !e.try_recv(out) {
        thread::yield_now();
    }
}

/// Two threads in one process, a heap ring each way.
pub fn thread_ring(n: usize, warmup: usize, wait: Wait) -> io::Result<Vec<u64>> {
    let cpus = peer_cpus();
    let (mut a, mut b) = Endpoint::heap_pair();
    let total = warmup + n;
    let peer = thread::spawn(move || {
        let _pin = PinGuard::pin(cpus.map(|c| c.1));
        ring_echo(&mut b, total, wait);
        Ok(())
    });
    let pin = PinGuard::pin(cpus.map(|c| c.0));
    let out = ring_initiate(&mut a, n, warmup, wait);
    drop(pin);
    join_peer(peer)?;
    Ok(out)
}

/// Two processes, the same ring in an anonymous shared mapping. The child is
/// forked from the calling process and exits when the run ends.
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub fn process_ring(n: usize, warmup: usize, wait: Wait) -> io::Result<Vec<u64>> {
    use crate::ring::{Duplex, duplex_size};
    use crate::sys::{Forked, exit_now, fork_process, map_shared, unmap, wait_child};

    let len = duplex_size();
    let p = map_shared(len)?;
    let cpus = peer_cpus();
    let total = warmup + n;
    // SAFETY: the child only touches the mapping and raw syscalls, then
    // `_exit`s; it never allocates or unwinds.
    match unsafe { fork_process() } {
        Err(e) => {
            unsafe { unmap(p, len) };
            Err(e)
        }
        Ok(Forked::Child) => {
            let _pin = PinGuard::pin(cpus.map(|c| c.1));
            let (_, mut b) = unsafe { Endpoint::mapped_pair(p as *const Duplex) };
            ring_echo(&mut b, total, wait);
            exit_now(0)
        }
        Ok(Forked::Parent(pid)) => {
            let pin = PinGuard::pin(cpus.map(|c| c.0));
            let (mut a, _) = unsafe { Endpoint::mapped_pair(p as *const Duplex) };
            let out = ring_initiate(&mut a, n, warmup, wait);
            drop(pin);
            let status = wait_child(pid);
            unsafe { unmap(p, len) };
            match status? {
                0 => Ok(out),
                s => Err(io::Error::other(format!(
                    "echo process exited with status {s}"
                ))),
            }
        }
    }
}

// ------------------------------------------------------------- streams

fn stream_initiate<S: Read + Write>(s: &mut S, n: usize, warmup: usize) -> io::Result<Vec<u64>> {
    let mut out = Vec::with_capacity(n);
    let mut back = [0u8; MSG_LEN];
    for i in 0..warmup + n {
        let m = message(i as u64);
        let t0 = Instant::now();
        s.write_all(&m)?;
        s.read_exact(&mut back)?;
        keep(i, warmup, t0, &mut out);
        debug_assert_eq!(back, m);
    }
    Ok(out)
}

fn stream_echo<S: Read + Write>(s: &mut S, total: usize) -> io::Result<()> {
    let mut m = [0u8; MSG_LEN];
    for _ in 0..total {
        s.read_exact(&mut m)?;
        s.write_all(&m)?;
    }
    Ok(())
}

/// A Unix domain stream socket pair, blocking reads and writes. The echo end
/// is a thread: the kernel path a message takes is the same whether the peer
/// is a thread or a process, once the two are on different cores.
#[cfg(unix)]
pub fn uds_stream(n: usize, warmup: usize) -> io::Result<Vec<u64>> {
    use std::os::unix::net::UnixStream;

    let (mut a, mut b) = UnixStream::pair()?;
    let cpus = peer_cpus();
    let total = warmup + n;
    let peer = thread::spawn(move || {
        let _pin = PinGuard::pin(cpus.map(|c| c.1));
        stream_echo(&mut b, total)
    });
    let pin = PinGuard::pin(cpus.map(|c| c.0));
    let out = stream_initiate(&mut a, n, warmup);
    drop(pin);
    join_peer(peer)?;
    out
}

/// TCP over 127.0.0.1 with `TCP_NODELAY` on both ends.
pub fn tcp_loopback(n: usize, warmup: usize) -> io::Result<Vec<u64>> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let addr = l.local_addr()?;
    let cpus = peer_cpus();
    let total = warmup + n;
    let peer = thread::spawn(move || {
        let _pin = PinGuard::pin(cpus.map(|c| c.1));
        let (mut s, _) = l.accept()?;
        s.set_nodelay(true)?;
        stream_echo(&mut s, total)
    });
    let pin = PinGuard::pin(cpus.map(|c| c.0));
    let mut s = TcpStream::connect(addr)?;
    s.set_nodelay(true)?;
    let out = stream_initiate(&mut s, n, warmup);
    drop(pin);
    join_peer(peer)?;
    out
}

/// An SSE stream over TCP loopback. The initiator is the server: it writes
/// each message as an event, and the client parses it and returns a raw
/// 64-byte ack on the same connection. The difference from
/// [`tcp_loopback`] is the HTTP handshake (outside the timed loop) and the
/// event framing and parsing (inside it).
pub fn sse_loopback(n: usize, warmup: usize) -> io::Result<Vec<u64>> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let addr = l.local_addr()?;
    let cpus = peer_cpus();
    let total = warmup + n;
    let peer = thread::spawn(move || -> io::Result<()> {
        let _pin = PinGuard::pin(cpus.map(|c| c.1));
        let mut s = TcpStream::connect(addr)?;
        s.set_nodelay(true)?;
        s.write_all(REQUEST_HEAD)?;
        let mut rd = SseReader::new(BufReader::new(s.try_clone()?));
        rd.skip_http_head()?;
        let mut data = Vec::with_capacity(MSG_LEN * 2);
        for _ in 0..total {
            if !rd.next_event(&mut data)? {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "stream ended early",
                ));
            }
            s.write_all(&data)?;
        }
        Ok(())
    });

    let pin = PinGuard::pin(cpus.map(|c| c.0));
    let (s, _) = l.accept()?;
    s.set_nodelay(true)?;
    let mut rd = BufReader::new(s.try_clone()?);
    let mut scratch = Vec::new();
    skip_head(&mut rd, &mut scratch)?;
    let mut w = SseWriter::new(s);
    w.get_mut().write_all(RESPONSE_HEAD)?;

    let mut out = Vec::with_capacity(n);
    let mut back = [0u8; MSG_LEN];
    for i in 0..total {
        let m = message(i as u64);
        let t0 = Instant::now();
        w.send(&m)?;
        rd.read_exact(&mut back)?;
        keep(i, warmup, t0, &mut out);
        debug_assert_eq!(back, m);
    }
    drop(pin);
    join_peer(peer)?;
    Ok(out)
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod tests;
