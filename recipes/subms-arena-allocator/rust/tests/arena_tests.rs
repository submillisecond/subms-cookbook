use subms_arena_allocator::Bump;

#[test]
fn alloc_and_read_back() {
    let mut a = Bump::with_capacity(256);
    let x = a.alloc_copy(42u32);
    assert_eq!(*x, 42);
    let y = a.alloc_copy(99u64);
    assert_eq!(*y, 99);
}

#[test]
fn reset_rewinds() {
    let mut a = Bump::with_capacity(128);
    for i in 0..16u32 {
        let _ = a.alloc_copy(i);
    }
    a.reset();
    // After reset we can fit a full new round in the same chunk.
    for i in 0..16u32 {
        let _ = a.alloc_copy(i);
    }
}

#[test]
fn fixed_capacity_refuses_when_full() {
    let mut a = Bump::with_capacity(64);
    // Fill it: 64 bytes / 8 = 8 u64s.
    for _ in 0..8u64 {
        let _ = a.alloc_copy(0u64);
    }
    // The 9th must fail via the fallible path.
    assert!(a.try_alloc_copy(0u64).is_none());
}

#[test]
fn alignment_is_respected() {
    let mut a = Bump::with_capacity(256);
    let _x = a.alloc_copy(1u8);
    let p = a.alloc_copy(0xdeadbeef_u64) as *mut u64 as usize;
    assert_eq!(p % 8, 0, "u64 must be 8-byte-aligned");
}

#[test]
fn many_resets_reuse_chunk() {
    let mut a = Bump::with_capacity(256);
    let cap = a.capacity();
    for _ in 0..1000 {
        for i in 0..30u8 {
            let _ = a.alloc_copy(i);
        }
        a.reset();
    }
    // Capacity is fixed - reset never reallocates.
    assert_eq!(a.capacity(), cap);
}

#[test]
fn alloc_default_constructor() {
    let mut a: Bump = Bump::default();
    let _x = a.alloc_copy(42u32);
    assert!(a.capacity() > 0);
}

#[test]
fn alloc_zero_size_allowed() {
    let mut a = Bump::with_capacity(64);
    #[derive(Copy, Clone)]
    struct Marker;
    let _ = a.alloc_copy(Marker);
}

#[test]
fn aligned_after_many_unaligned() {
    let mut a = Bump::with_capacity(1024);
    for _ in 0..10 {
        let _ = a.alloc_copy(1u8);
    }
    let p = a.alloc_copy(0xff_u32) as *mut u32 as usize;
    assert_eq!(
        p % 4,
        0,
        "u32 must be 4-byte aligned even after byte-stream"
    );
}

#[test]
fn read_back_matches_written() {
    let mut a = Bump::with_capacity(256);
    let v1 = *a.alloc_copy(123u32);
    let v2 = *a.alloc_copy(456u32);
    assert_eq!(v1, 123);
    assert_eq!(v2, 456);
}

#[test]
fn used_tracks_cursor() {
    let mut a = Bump::with_capacity(256);
    assert_eq!(a.used(), 0);
    let _ = a.alloc_copy(0u64);
    assert_eq!(a.used(), 8);
    a.reset();
    assert_eq!(a.used(), 0);
}
