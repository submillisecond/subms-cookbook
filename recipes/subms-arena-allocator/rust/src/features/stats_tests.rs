use super::*;

#[test]
fn allocations_counter_increments() {
    let mut a = StatsBump::with_capacity(256);
    for i in 0..5u32 {
        let _ = a.alloc_copy(i);
    }
    assert_eq!(a.stats().allocations, 5);
}

#[test]
fn bytes_used_sums_sizes() {
    let mut a = StatsBump::with_capacity(256);
    let _ = a.alloc_copy(1u8); // 1
    let _ = a.alloc_copy(1u32); // 4
    let _ = a.alloc_copy(1u64); // 8
    assert_eq!(a.stats().bytes_used, 1 + 4 + 8);
}

#[test]
fn bytes_wasted_tracks_padding() {
    let mut a = StatsBump::with_capacity(256);
    // After a 1-byte alloc the cursor is at 1. A u32 forces 3
    // bytes of padding to reach offset 4. A u64 right after lands
    // at offset 8 (no padding because cursor is already at 8).
    let _ = a.alloc_copy(1u8);
    let _ = a.alloc_copy(1u32);
    let _ = a.alloc_copy(1u64);
    assert_eq!(a.stats().bytes_wasted, 3);
}

#[test]
fn peak_persists_across_reset() {
    let mut a = StatsBump::with_capacity(1024);
    for _ in 0..50u64 {
        let _ = a.alloc_copy(0u64);
    }
    let peak_before = a.stats().peak_bytes;
    assert!(peak_before >= 50 * 8);
    a.reset();
    let _ = a.alloc_copy(0u64);
    assert_eq!(a.stats().peak_bytes, peak_before, "peak survives reset");
}

#[test]
fn chunk_count_increments_on_grow() {
    let mut a = StatsBump::with_capacity(64);
    assert_eq!(a.stats().chunk_count, 1);
    for _ in 0..32u64 {
        let _ = a.alloc_copy(0u64);
    }
    assert!(a.stats().chunk_count >= 2, "grow must bump chunk count");
}

#[test]
fn clear_stats_zeroes_counters() {
    let mut a = StatsBump::with_capacity(256);
    for _ in 0..10u64 {
        let _ = a.alloc_copy(0u64);
    }
    assert!(a.stats().allocations > 0);
    a.clear_stats();
    let s = a.stats();
    assert_eq!(s.allocations, 0);
    assert_eq!(s.bytes_used, 0);
    assert_eq!(s.bytes_wasted, 0);
    assert_eq!(s.peak_bytes, 0);
    assert_eq!(s.chunk_count, 1);
}
