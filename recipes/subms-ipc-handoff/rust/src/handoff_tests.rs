use super::*;

const N: usize = 200;
const W: usize = 20;

fn sane(s: &[u64]) {
    assert_eq!(s.len(), N);
    assert!(s.iter().all(|&ns| ns < 5_000_000_000));
}

#[test]
fn message_is_printable_and_sequenced() {
    let m = message(0xab);
    assert_eq!(&m[..16], b"00000000000000ab");
    assert!(m.iter().all(|b| b.is_ascii_graphic()));
    assert!(!m.contains(&b'\n'));
}

#[test]
fn thread_ring_spin() {
    sane(&thread_ring(N, W, Wait::Spin).unwrap());
}

#[test]
fn thread_ring_park() {
    sane(&thread_ring(N, W, Wait::Park).unwrap());
}

#[test]
fn tcp() {
    sane(&tcp_loopback(N, W).unwrap());
}

#[test]
fn sse() {
    sane(&sse_loopback(N, W).unwrap());
}

#[cfg(unix)]
#[test]
fn uds() {
    sane(&uds_stream(N, W).unwrap());
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn process_ring_spin_and_park() {
    sane(&process_ring(N, W, Wait::Spin).unwrap());
    sane(&process_ring(N, W, Wait::Park).unwrap());
}
