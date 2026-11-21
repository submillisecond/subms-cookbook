//! Fixed-size message ring laid out as plain memory. All-zero bytes are a
//! valid empty state, so the same struct works boxed on the heap between two
//! threads or in an anonymous shared mapping between two processes.

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Bytes per message. One cache line on x86 and most ARM servers.
pub const MSG_LEN: usize = 64;
/// One message.
pub type Msg = [u8; MSG_LEN];
/// Slots per ring. A ping-pong keeps one message in flight, so depth only
/// matters to callers that pipeline.
pub const SLOTS: usize = 64;
const MASK: u64 = SLOTS as u64 - 1;

// 128 rather than 64: adjacent-line prefetchers pull line pairs, so a 64-byte
// gap can still put the producer's and consumer's counters in one fetch.
#[repr(C, align(128))]
struct Pad64(AtomicU64);
#[repr(C, align(128))]
struct Pad32(AtomicU32);

/// One direction of a [`Duplex`]. Single producer, single consumer.
#[repr(C)]
pub struct Ring {
    head: Pad64,
    tail: Pad64,
    /// Bumped after every send that may need a wakeup; the futex word a parked
    /// receiver sleeps on.
    seq: Pad32,
    waiting: Pad32,
    slots: [UnsafeCell<Msg>; SLOTS],
}

// The two ends touch disjoint fields except through the atomics; slot access
// is ordered by the head/tail Release/Acquire pair.
unsafe impl Sync for Ring {}
unsafe impl Send for Ring {}

impl Ring {
    fn zeroed() -> Self {
        Ring {
            head: Pad64(AtomicU64::new(0)),
            tail: Pad64(AtomicU64::new(0)),
            seq: Pad32(AtomicU32::new(0)),
            waiting: Pad32(AtomicU32::new(0)),
            slots: std::array::from_fn(|_| UnsafeCell::new([0; MSG_LEN])),
        }
    }

    fn try_send(&self, m: &Msg) -> bool {
        let tail = self.tail.0.load(Ordering::Relaxed);
        let head = self.head.0.load(Ordering::Acquire);
        if tail - head == SLOTS as u64 {
            return false;
        }
        unsafe { *self.slots[(tail & MASK) as usize].get() = *m };
        self.tail.0.store(tail + 1, Ordering::Release);
        true
    }

    fn try_recv(&self, out: &mut Msg) -> bool {
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Acquire);
        if head == tail {
            return false;
        }
        *out = unsafe { *self.slots[(head & MASK) as usize].get() };
        self.head.0.store(head + 1, Ordering::Release);
        true
    }

    fn len(&self) -> usize {
        let tail = self.tail.0.load(Ordering::Acquire);
        let head = self.head.0.load(Ordering::Acquire);
        (tail - head) as usize
    }
}

/// Two rings, one per direction. `#[repr(C)]` so a forked child and its
/// parent agree on the layout of the mapping they share.
#[repr(C)]
pub struct Duplex {
    ping: Ring,
    pong: Ring,
}

impl Duplex {
    fn zeroed() -> Self {
        Duplex {
            ping: Ring::zeroed(),
            pong: Ring::zeroed(),
        }
    }
}

enum Holder {
    Heap(Arc<Duplex>),
    Mapped(*const Duplex),
}

#[derive(Clone, Copy)]
enum Side {
    A,
    B,
}

/// One end of a duplex: sends on one ring, receives on the other. Not
/// `Clone`, and every operation takes `&mut self`, which is what keeps each
/// ring single-producer single-consumer.
pub struct Endpoint {
    holder: Holder,
    side: Side,
}

// A Mapped endpoint points into a shared mapping whose lifetime the creator
// of the pair guarantees (see `mapped_pair`).
unsafe impl Send for Endpoint {}

impl Endpoint {
    /// Both ends of a duplex on the heap, for two threads.
    pub fn heap_pair() -> (Endpoint, Endpoint) {
        let d = Arc::new(Duplex::zeroed());
        (
            Endpoint {
                holder: Holder::Heap(Arc::clone(&d)),
                side: Side::A,
            },
            Endpoint {
                holder: Holder::Heap(d),
                side: Side::B,
            },
        )
    }

    /// Both ends of a duplex living at `p`, typically a shared mapping of
    /// [`duplex_size`] bytes set up before `fork`.
    ///
    /// # Safety
    /// `p` must point to zeroed or previously-initialised `Duplex` memory that
    /// outlives both endpoints, and each end must be used by one thread or
    /// process only.
    pub unsafe fn mapped_pair(p: *const Duplex) -> (Endpoint, Endpoint) {
        (
            Endpoint {
                holder: Holder::Mapped(p),
                side: Side::A,
            },
            Endpoint {
                holder: Holder::Mapped(p),
                side: Side::B,
            },
        )
    }

    fn duplex(&self) -> &Duplex {
        match &self.holder {
            Holder::Heap(d) => d,
            Holder::Mapped(p) => unsafe { &**p },
        }
    }

    fn tx(&self) -> &Ring {
        let d = self.duplex();
        match self.side {
            Side::A => &d.ping,
            Side::B => &d.pong,
        }
    }

    fn rx(&self) -> &Ring {
        let d = self.duplex();
        match self.side {
            Side::A => &d.pong,
            Side::B => &d.ping,
        }
    }

    /// Send without waking a parked receiver. `false` when the ring is full.
    pub fn try_send(&mut self, m: &Msg) -> bool {
        self.tx().try_send(m)
    }

    /// Receive one message if there is one.
    pub fn try_recv(&mut self, out: &mut Msg) -> bool {
        self.rx().try_recv(out)
    }

    /// Messages waiting on this end's receive ring.
    pub fn pending(&self) -> usize {
        self.rx().len()
    }

    /// Busy-spin until a message arrives.
    pub fn recv_spin(&mut self, out: &mut Msg) {
        while !self.rx().try_recv(out) {
            std::hint::spin_loop();
        }
    }

    /// Send, and wake the receiver if it is parked. `false` when full.
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    pub fn send_wake(&mut self, m: &Msg) -> bool {
        let r = self.tx();
        if !r.try_send(m) {
            return false;
        }
        r.seq.0.fetch_add(1, Ordering::SeqCst);
        // Pairs with the fence in `recv_park`: either this load sees the
        // receiver's `waiting = 1`, or the receiver's recheck sees the tail.
        std::sync::atomic::fence(Ordering::SeqCst);
        if r.waiting.0.load(Ordering::SeqCst) != 0 {
            crate::sys::futex_wake(&r.seq.0);
        }
        true
    }

    /// Park on a futex until a message arrives. Works across processes
    /// because the futex is the non-private kind, keyed on the shared page.
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    pub fn recv_park(&mut self, out: &mut Msg) {
        let r = self.rx();
        loop {
            if r.try_recv(out) {
                return;
            }
            let s = r.seq.0.load(Ordering::SeqCst);
            r.waiting.0.store(1, Ordering::SeqCst);
            std::sync::atomic::fence(Ordering::SeqCst);
            if r.try_recv(out) {
                r.waiting.0.store(0, Ordering::Relaxed);
                return;
            }
            crate::sys::futex_wait(&r.seq.0, s);
            r.waiting.0.store(0, Ordering::Relaxed);
        }
    }
}

/// Bytes a [`Duplex`] occupies, for callers sizing a shared mapping.
pub const fn duplex_size() -> usize {
    std::mem::size_of::<Duplex>()
}

#[cfg(test)]
#[path = "ring_tests.rs"]
mod tests;
