use super::*;

fn msg(b: u8) -> Msg {
    [b; MSG_LEN]
}

#[test]
fn delivers_in_order_each_way() {
    let (mut a, mut b) = Endpoint::heap_pair();
    let mut got = [0u8; MSG_LEN];
    assert!(a.try_send(&msg(1)));
    assert!(a.try_send(&msg(2)));
    assert!(b.try_send(&msg(9)));
    assert_eq!(b.pending(), 2);
    assert!(b.try_recv(&mut got));
    assert_eq!(got, msg(1));
    assert!(b.try_recv(&mut got));
    assert_eq!(got, msg(2));
    assert!(!b.try_recv(&mut got));
    assert!(a.try_recv(&mut got));
    assert_eq!(got, msg(9));
    assert!(!a.try_recv(&mut got));
}

#[test]
fn full_ring_refuses_then_wraps() {
    let (mut a, mut b) = Endpoint::heap_pair();
    let mut got = [0u8; MSG_LEN];
    for i in 0..SLOTS {
        assert!(a.try_send(&msg(i as u8)));
    }
    assert!(!a.try_send(&msg(0xff)));
    assert_eq!(b.pending(), SLOTS);
    // Three laps of the index mask.
    for lap in 0..3 * SLOTS {
        assert!(b.try_recv(&mut got));
        assert_eq!(got, msg((lap % 256) as u8));
        assert!(a.try_send(&msg(((lap + SLOTS) % 256) as u8)));
    }
}

#[test]
fn spin_receive_across_threads() {
    let (mut a, mut b) = Endpoint::heap_pair();
    let t = std::thread::spawn(move || {
        let mut m = [0u8; MSG_LEN];
        for _ in 0..1000 {
            b.recv_spin(&mut m);
            while !b.try_send(&m) {}
        }
    });
    let mut back = [0u8; MSG_LEN];
    for i in 0..1000u32 {
        let mut m = [0u8; MSG_LEN];
        m[..4].copy_from_slice(&i.to_le_bytes());
        assert!(a.try_send(&m));
        a.recv_spin(&mut back);
        assert_eq!(back, m);
    }
    t.join().unwrap();
}

#[test]
fn mapped_pair_over_caller_memory() {
    let d = Box::new(Duplex::zeroed());
    let p: *const Duplex = &*d;
    let (mut a, mut b) = unsafe { Endpoint::mapped_pair(p) };
    let mut got = [0u8; MSG_LEN];
    assert!(a.try_send(&msg(5)));
    assert!(b.try_recv(&mut got));
    assert_eq!(got, msg(5));
    drop((a, b));
    drop(d);
}

#[test]
fn layout_is_cache_line_separated() {
    assert_eq!(std::mem::align_of::<Ring>(), 128);
    assert_eq!(duplex_size() % 128, 0);
    assert!(duplex_size() >= 2 * (4 * 128 + SLOTS * MSG_LEN));
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn park_and_wake_across_threads() {
    let (mut a, mut b) = Endpoint::heap_pair();
    let t = std::thread::spawn(move || {
        let mut m = [0u8; MSG_LEN];
        for _ in 0..2000 {
            b.recv_park(&mut m);
            while !b.send_wake(&m) {}
        }
    });
    let mut back = [0u8; MSG_LEN];
    for i in 0..2000u32 {
        let mut m = [0u8; MSG_LEN];
        m[..4].copy_from_slice(&i.to_le_bytes());
        assert!(a.send_wake(&m));
        a.recv_park(&mut back);
        assert_eq!(back, m);
    }
    t.join().unwrap();
}
