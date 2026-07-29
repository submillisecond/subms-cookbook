use super::*;

#[test]
fn fresh_alloc_bumps_cursor() {
    let mut a = FreelistBump::with_capacity(256);
    let _ = a.alloc_copy(123u32);
    assert!(a.used() >= 4);
    assert_eq!(a.reuse_hits(), 0);
}

#[test]
fn freed_slot_is_reused() {
    let mut a = FreelistBump::with_capacity(256);
    let layout = Layout::new::<u32>();
    let p1 = a.alloc_raw(layout);
    let used_after_first = a.used();
    unsafe { a.free(p1, layout) };
    let p2 = a.alloc_raw(layout);
    assert_eq!(p1, p2, "freed slot must be reused");
    assert_eq!(
        a.used(),
        used_after_first,
        "reusing should not advance cursor"
    );
    assert_eq!(a.reuse_hits(), 1);
}

#[test]
fn freelist_is_per_size() {
    let mut a = FreelistBump::with_capacity(256);
    let l32 = Layout::new::<u32>();
    let l64 = Layout::new::<u64>();
    let p32 = a.alloc_raw(l32);
    unsafe { a.free(p32, l32) };
    // A different-size alloc must not steal the freed slot.
    let p64 = a.alloc_raw(l64);
    assert_ne!(p32, p64);
    assert_eq!(a.reuse_hits(), 0);
}

#[test]
fn many_frees_reused_lifo() {
    let mut a = FreelistBump::with_capacity(1024);
    let layout = Layout::new::<u64>();
    let p1 = a.alloc_raw(layout);
    let p2 = a.alloc_raw(layout);
    let p3 = a.alloc_raw(layout);
    unsafe {
        a.free(p1, layout);
        a.free(p2, layout);
        a.free(p3, layout);
    }
    assert_eq!(a.freelist_len(), 3);
    // LIFO: last freed is first reused.
    let r1 = a.alloc_raw(layout);
    let r2 = a.alloc_raw(layout);
    let r3 = a.alloc_raw(layout);
    assert_eq!(r1, p3);
    assert_eq!(r2, p2);
    assert_eq!(r3, p1);
    assert_eq!(a.reuse_hits(), 3);
    assert_eq!(a.freelist_len(), 0);
}

#[test]
fn reset_clears_freelist_and_cursor() {
    let mut a = FreelistBump::with_capacity(256);
    let layout = Layout::new::<u32>();
    let p = a.alloc_raw(layout);
    unsafe { a.free(p, layout) };
    assert_eq!(a.freelist_len(), 1);
    a.reset();
    assert_eq!(a.freelist_len(), 0);
    assert_eq!(a.used(), 0);
    assert_eq!(a.reuse_hits(), 0);
}

#[test]
fn reuse_under_64_byte_alignment() {
    // Free + reuse a cache-line slot. Alignment must round-trip.
    let mut a = FreelistBump::with_capacity(512);
    let layout = Layout::from_size_align(64, 64).expect("layout");
    let p1 = a.alloc_raw(layout);
    assert_eq!(p1 as usize % 64, 0);
    unsafe { a.free(p1, layout) };
    let p2 = a.alloc_raw(layout);
    assert_eq!(p1, p2);
    assert_eq!(p2 as usize % 64, 0);
}
