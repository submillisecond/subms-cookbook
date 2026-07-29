use super::*;

#[test]
fn round_trip_below_saturation() {
    let mut cf = CompressedCuckooFilter::with_capacity(1000);
    for i in 0..500u32 {
        assert!(cf.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.contains(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.delete(&format!("k{i}")));
    }
    assert_eq!(cf.len(), 0);
}

#[test]
fn empty_filter_rejects_everything() {
    let cf = CompressedCuckooFilter::with_capacity(100);
    assert!(!cf.contains("never-inserted"));
    assert!(cf.is_empty());
    assert_eq!(cf.len(), 0);
}

#[test]
fn occupied_bytes_grows_with_inserts() {
    let mut cf = CompressedCuckooFilter::with_capacity(500);
    let baseline = cf.occupied_bytes();
    for i in 0..200u32 {
        cf.insert(&format!("k{i}"));
    }
    assert!(cf.occupied_bytes() > baseline, "expected occupancy to grow");
}

#[test]
fn delete_unknown_returns_false() {
    let mut cf = CompressedCuckooFilter::with_capacity(100);
    cf.insert("known");
    assert!(!cf.delete("never-inserted"));
    assert!(cf.contains("known"));
}

#[test]
fn sorted_invariant_holds_through_inserts_and_deletes() {
    // Whitebox: after mixed ops every bucket's run must be sorted
    // ascending.
    let mut cf = CompressedCuckooFilter::with_capacity(500);
    for i in 0..400u32 {
        cf.insert(&format!("k{i}"));
    }
    for i in 0..200u32 {
        cf.delete(&format!("k{i}"));
    }
    for i in 400..500u32 {
        cf.insert(&format!("k{i}"));
    }
    for bucket in &cf.buckets {
        let count = bucket[0] as usize;
        for k in 1..count {
            assert!(
                bucket[1 + k - 1] <= bucket[1 + k],
                "bucket out of order: {bucket:?}"
            );
        }
    }
}

#[test]
fn false_positive_rate_in_three_percent_range() {
    let n = 5_000usize;
    let mut cf = CompressedCuckooFilter::with_capacity(n);
    for i in 0..n {
        cf.insert(&format!("present{i}"));
    }
    let probes = 10_000usize;
    let mut fp = 0usize;
    for i in 0..probes {
        if cf.contains(&format!("absent{i}")) {
            fp += 1;
        }
    }
    let fpr = fp as f64 / probes as f64;
    assert!(fpr < 0.03, "fpr {fpr:.4} too high");
}

#[test]
fn bucket_count_is_power_of_two() {
    let cf = CompressedCuckooFilter::with_capacity(1000);
    assert!(cf.bucket_count().is_power_of_two());
}

#[test]
fn duplicate_inserts_stack_in_bucket() {
    let mut cf = CompressedCuckooFilter::with_capacity(100);
    cf.insert("dup");
    cf.insert("dup");
    cf.insert("dup");
    assert_eq!(cf.len(), 3);
    assert!(cf.contains("dup"));
    cf.delete("dup");
    cf.delete("dup");
    cf.delete("dup");
    assert!(!cf.contains("dup"));
}
