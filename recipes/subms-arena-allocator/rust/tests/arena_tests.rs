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
fn grows_when_chunk_full() {
    let mut a = Bump::with_capacity(64);
    let cap_before = a.total_capacity();
    // 64-byte chunk + u64 each = 8 fits cleanly. Force growth.
    for _ in 0..32u64 {
        let _ = a.alloc_copy(0u64);
    }
    assert!(a.total_capacity() > cap_before, "should have grown");
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
    let cap = a.total_capacity();
    for _ in 0..1000 {
        for i in 0..30u8 { let _ = a.alloc_copy(i); }
        a.reset();
    }
    // After many reset cycles within capacity, no new chunk should be needed.
    assert_eq!(a.total_capacity(), cap);
}

#[test]
fn alloc_default_constructor() {
    let mut a: Bump = Bump::default();
    let _x = a.alloc_copy(42u32);
    assert!(a.total_capacity() > 0);
}

#[test]
fn alloc_zero_size_allowed() {
    let mut a = Bump::with_capacity(64);
    // Allocate empty marker struct.
    #[derive(Copy, Clone)]
    struct Marker;
    let _ = a.alloc_copy(Marker);
}

#[test]
fn aligned_after_many_unaligned() {
    let mut a = Bump::with_capacity(1024);
    for _ in 0..10 { let _ = a.alloc_copy(1u8); }
    let p = a.alloc_copy(0xff_u32) as *mut u32 as usize;
    assert_eq!(p % 4, 0, "u32 must be 4-byte aligned even after byte-stream");
}

#[test]
fn very_large_allocation_grows_appropriately() {
    let mut a = Bump::with_capacity(64);
    // Force allocation larger than initial chunk.
    #[derive(Copy, Clone)]
    struct Big([u8; 256]);
    let _b = a.alloc_copy(Big([0u8; 256]));
    assert!(a.total_capacity() >= 256);
}

#[test]
fn read_back_matches_written() {
    let mut a = Bump::with_capacity(256);
    let v1 = *a.alloc_copy(123u32);
    let v2 = *a.alloc_copy(456u32);
    assert_eq!(v1, 123);
    assert_eq!(v2, 456);
}
