use super::*;

#[test]
fn alignment_one_packs_tightly() {
    let mut a = AlignedBump::with_capacity(256);
    {
        let s1 = a.alloc_aligned(3, 1);
        assert_eq!(s1.len(), 3);
    }
    {
        let s2 = a.alloc_aligned(3, 1);
        assert_eq!(s2.len(), 3);
    }
    // Two byte-aligned 3-byte regions take 6 bytes total.
    assert_eq!(a.used(), 6);
}

#[test]
fn alignment_64_byte_cache_line() {
    let mut a = AlignedBump::with_capacity(512);
    // Burn one byte so the next alloc has to pad.
    let _ = a.alloc_aligned(1, 1);
    let s = a.alloc_aligned(64, 64);
    let p = s.as_ptr() as usize;
    assert_eq!(p % 64, 0, "cache-line alignment");
}

#[test]
fn alignment_512_byte_page_ish() {
    // Higher-than-base alignment: exercise the padding math.
    let mut a = AlignedBump::with_capacity(4096);
    let _ = a.alloc_aligned(7, 1);
    let s = a.alloc_aligned(128, 512);
    let p = s.as_ptr() as usize;
    assert_eq!(p % 512, 0);
}

#[test]
fn rejects_non_power_of_two_align() {
    let mut a = AlignedBump::with_capacity(64);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Drop the returned slice immediately so it doesn't escape.
        let _ = a.alloc_aligned(8, 3);
    }));
    assert!(r.is_err());
}

#[test]
fn out_of_capacity_returns_none() {
    let mut a = AlignedBump::with_capacity(64);
    // Capacity is at least 64; 64 + 64 will overflow.
    let _ = a.alloc_aligned(64, 1);
    assert!(a.try_alloc_aligned(64, 64).is_none());
}

#[test]
#[should_panic(expected = "out of capacity")]
fn alloc_aligned_panics_when_full() {
    let mut a = AlignedBump::with_capacity(64);
    let _ = a.alloc_aligned(64, 1);
    // No room for a second 64-byte region -> panics.
    let _ = a.alloc_aligned(64, 1);
}

#[test]
fn capacity_reports_backing_size() {
    let a = AlignedBump::with_capacity(200);
    // Capacity is the (>= 64) chunk size honoured by the allocator.
    assert!(a.capacity() >= 200);
    assert_eq!(a.used(), 0);
}

#[test]
fn reset_rewinds_cursor() {
    let mut a = AlignedBump::with_capacity(128);
    let _ = a.alloc_aligned(64, 64);
    assert_eq!(a.used(), 64);
    a.reset();
    assert_eq!(a.used(), 0);
    let _ = a.alloc_aligned(64, 64);
    assert_eq!(a.used(), 64);
}

#[test]
fn slice_is_writable() {
    let mut a = AlignedBump::with_capacity(256);
    let s = a.alloc_aligned(16, 8);
    for (i, b) in s.iter_mut().enumerate() {
        *b = i as u8;
    }
    // Re-borrow via fresh alloc would alias, so check ptr equality
    // by re-walking the cursor difference. Simpler: just confirm
    // the writes stuck via the same slice ref.
    for (i, b) in s.iter().enumerate() {
        assert_eq!(*b, i as u8);
    }
}
