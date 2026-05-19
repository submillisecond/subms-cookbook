use subms_cuckoo_filter::CuckooFilter;

#[test]
fn insert_contains_delete_round_trip() {
    let mut cf = CuckooFilter::with_capacity(1000);
    for i in 0..500u32 {
        assert!(cf.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.contains(&format!("k{i}")), "k{i} should be present");
    }
    for i in 0..500u32 {
        assert!(cf.delete(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(!cf.contains(&format!("k{i}")), "k{i} should be gone");
    }
    assert_eq!(cf.len(), 0);
}

#[test]
fn delete_nonexistent_returns_false() {
    let mut cf = CuckooFilter::with_capacity(100);
    assert!(!cf.delete("never-inserted"));
}

#[test]
fn empty_contains_returns_false() {
    let cf = CuckooFilter::with_capacity(100);
    assert!(!cf.contains("anything"));
}

#[test]
fn false_positive_rate_under_three_percent() {
    let n = 10_000;
    let mut cf = CuckooFilter::with_capacity(n);
    for i in 0..n {
        assert!(cf.insert(&format!("present{i}")));
    }
    let probes = 10_000;
    let mut fp = 0usize;
    for i in 0..probes {
        if cf.contains(&format!("absent{i}")) {
            fp += 1;
        }
    }
    let fpr = fp as f64 / probes as f64;
    // Cuckoo with 8-bit fingerprints + bucket 4 lands around 0.3% theoretical;
    // 3% is generous headroom for the FNV+SplitMix hash combo.
    assert!(fpr < 0.03, "fpr {fpr:.4} too high");
}

#[test]
fn bucket_count_is_power_of_two() {
    let cf = CuckooFilter::with_capacity(1000);
    let n = cf.bucket_count();
    assert!(n.is_power_of_two(), "{n} should be power of 2");
}

#[test]
fn len_tracks_inserts_and_deletes() {
    let mut cf = CuckooFilter::with_capacity(1000);
    assert_eq!(cf.len(), 0);
    cf.insert("a");
    cf.insert("b");
    assert_eq!(cf.len(), 2);
    cf.delete("a");
    assert_eq!(cf.len(), 1);
    cf.delete("absent");
    assert_eq!(cf.len(), 1);
}

#[test]
fn is_empty_initially() {
    let cf = CuckooFilter::with_capacity(100);
    assert!(cf.is_empty());
}

#[test]
fn duplicate_insert_increases_count() {
    // Cuckoo allows multiple entries per key (up to bucket capacity).
    let mut cf = CuckooFilter::with_capacity(100);
    cf.insert("dup");
    cf.insert("dup");
    cf.insert("dup");
    assert_eq!(cf.len(), 3);
    assert!(cf.contains("dup"));
    cf.delete("dup");
    cf.delete("dup");
    cf.delete("dup");
    assert!(!cf.contains("dup"));
    assert_eq!(cf.len(), 0);
}

#[test]
fn default_constructor_via_with_capacity_zero() {
    let cf = CuckooFilter::with_capacity(0);
    assert!(cf.bucket_count() >= 2);
}

#[test]
fn stress_insert_contains_delete_cycle() {
    let mut cf = CuckooFilter::with_capacity(2000);
    for cycle in 0..3 {
        for i in 0..1000 {
            cf.insert(&format!("cycle{cycle}-k{i}"));
        }
        for i in 0..1000 {
            assert!(cf.contains(&format!("cycle{cycle}-k{i}")));
        }
        for i in 0..1000 {
            cf.delete(&format!("cycle{cycle}-k{i}"));
        }
    }
    assert_eq!(cf.len(), 0);
}
