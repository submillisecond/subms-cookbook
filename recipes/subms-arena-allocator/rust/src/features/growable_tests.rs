use super::*;

#[test]
fn allocates_and_reads_back() {
    let mut a = GrowableBump::with_capacity(256);
    let r = a.alloc_copy(123u32);
    assert_eq!(*r, 123);
}

#[test]
fn grows_at_chunk_boundary() {
    let mut a = GrowableBump::with_capacity(64);
    let cap_before = a.total_capacity();
    // 64 bytes = 8 u64s. Push 32 to force at least one grow.
    for _ in 0..32u64 {
        let _ = a.alloc_copy(0u64);
    }
    assert!(
        a.total_capacity() > cap_before,
        "expected growth, got {} >= {}",
        cap_before,
        a.total_capacity()
    );
    assert!(a.chunk_count() >= 2);
}

#[test]
fn large_single_allocation_triggers_grow() {
    let mut a = GrowableBump::with_capacity(64);
    #[derive(Copy, Clone)]
    #[allow(dead_code)]
    struct Big([u8; 256]);
    let _b = a.alloc_copy(Big([0u8; 256]));
    assert!(a.total_capacity() >= 256);
}

#[test]
fn reset_keeps_largest_chunk_only() {
    let mut a = GrowableBump::with_capacity(64);
    for _ in 0..1024u64 {
        let _ = a.alloc_copy(0u64);
    }
    let chunks_before_reset = a.chunk_count();
    assert!(chunks_before_reset >= 2, "should have grown");
    a.reset();
    assert_eq!(a.chunk_count(), 1, "reset keeps one chunk");
    // The kept chunk is large enough to serve the next round.
    for _ in 0..16u64 {
        let _ = a.alloc_copy(0u64);
    }
}

#[test]
fn alignment_respected_across_grow() {
    let mut a = GrowableBump::with_capacity(64);
    // Force a grow by overfilling, then verify the next u64 still
    // lands on an 8-byte boundary inside the new chunk.
    for _ in 0..16u64 {
        let _ = a.alloc_copy(0u64);
    }
    let p = a.alloc_copy(0xdeadbeef_u64) as *mut u64 as usize;
    assert_eq!(p % 8, 0, "u64 must be 8-byte aligned post-grow");
}

#[test]
fn many_resets_no_chunk_churn() {
    // After the steady state settles on a single sufficient chunk,
    // subsequent resets must not allocate new chunks.
    let mut a = GrowableBump::with_capacity(256);
    for _ in 0..3 {
        for _ in 0..32u64 {
            let _ = a.alloc_copy(0u64);
        }
        a.reset();
    }
    let stable = a.total_capacity();
    for _ in 0..50 {
        for _ in 0..32u64 {
            let _ = a.alloc_copy(0u64);
        }
        a.reset();
    }
    assert_eq!(stable, a.total_capacity());
}
