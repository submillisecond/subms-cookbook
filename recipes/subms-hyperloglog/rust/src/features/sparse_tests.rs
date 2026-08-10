use super::*;

#[test]
fn starts_sparse_and_grows_linearly() {
    let mut h = SparseHyperLogLog::new(10);
    assert!(h.is_sparse());
    for i in 0..20 {
        h.add(&format!("k{i}"));
    }
    assert!(h.is_sparse(), "still sparse below threshold");
    assert!(h.entry_count() > 0);
}

#[test]
fn promotes_to_dense_at_threshold() {
    // p=8 -> m=256, default threshold = 64.
    let mut h = SparseHyperLogLog::new(8);
    // Push well past the threshold so promotion definitely fires.
    for i in 0..500 {
        h.add(&format!("k{i}"));
    }
    assert!(!h.is_sparse(), "promoted past threshold");
    assert!(h.as_dense().is_some());
}

#[test]
fn estimate_matches_dense_after_promotion() {
    let mut sparse = SparseHyperLogLog::new(10);
    let mut dense = HyperLogLog::new(10);
    for i in 0..2_000 {
        let k = format!("user-{i}");
        sparse.add(&k);
        dense.add(&k);
    }
    let s = sparse.estimate();
    let d = dense.estimate();
    let rel = ((s - d).abs() / d.max(1.0)).abs();
    assert!(
        rel < 0.05,
        "post-promotion match within 5%: sparse={s} dense={d}"
    );
}

#[test]
fn low_cardinality_accurate_via_linear_counting() {
    let mut h = SparseHyperLogLog::new(10);
    for i in 0..50 {
        h.add(&format!("k{i}"));
    }
    let est = h.estimate();
    assert!(
        est > 40.0 && est < 60.0,
        "low-card linear counting: got {est}"
    );
}

#[test]
fn with_threshold_controls_promotion_and_reports_shape() {
    // An explicit threshold of 4 promotes far sooner than the default m/4.
    let mut h = SparseHyperLogLog::with_threshold(10, 4);
    assert_eq!(h.precision(), 10);
    assert_eq!(h.register_count(), 1024);
    assert!(h.is_sparse());
    for i in 0..4 {
        h.add(&format!("k{i}"));
    }
    assert!(!h.is_sparse(), "promoted at the explicit threshold of 4");
}

#[test]
fn force_promote_idempotent() {
    let mut h = SparseHyperLogLog::new(8);
    h.add("a");
    h.promote();
    assert!(!h.is_sparse());
    h.promote();
    assert!(!h.is_sparse());
}

#[test]
fn duplicate_keys_dont_inflate_entry_count() {
    let mut h = SparseHyperLogLog::new(10);
    for _ in 0..1_000 {
        h.add("same-key");
    }
    assert!(h.is_sparse());
    assert_eq!(
        h.entry_count(),
        1,
        "one register touched, regardless of insert count"
    );
}

#[test]
fn clear_returns_a_promoted_sketch_to_sparse() {
    let mut h = SparseHyperLogLog::with_threshold(10, 8);
    for i in 0..50 {
        h.add(&format!("k{i}"));
    }
    assert!(!h.is_sparse());
    h.clear();
    assert!(h.is_sparse());
    assert!(h.is_empty());
    assert_eq!(h.entry_count(), 0);
    assert_eq!(h.threshold(), 8, "clear keeps the sizing decision");
}

#[test]
fn state_bytes_stays_far_under_dense_while_thin() {
    let mut h = SparseHyperLogLog::new(14);
    for i in 0..20 {
        h.add(&format!("cpty-{i}"));
    }
    assert!(h.is_sparse());
    assert!(
        h.state_bytes() < 16_384 / 10,
        "a thin sketch must not approach the 16 KB dense cost, got {}",
        h.state_bytes()
    );
    h.promote();
    assert_eq!(h.state_bytes(), 16_384);
}

#[test]
fn merge_of_two_sparse_sketches_unions_them() {
    let mut a = SparseHyperLogLog::with_threshold(12, 4096);
    let mut b = SparseHyperLogLog::with_threshold(12, 4096);
    for i in 0..300 {
        a.add(&format!("k{i}"));
        b.add(&format!("k{}", i + 200));
    }
    a.merge(&b).unwrap();
    assert!(a.is_sparse(), "500 entries stays under a 4096 threshold");
    let est = a.estimate();
    assert!(est > 450.0 && est < 550.0, "500 distinct, got {est}");
}

#[test]
fn merge_can_push_a_sparse_sketch_over_the_threshold() {
    let mut a = SparseHyperLogLog::with_threshold(12, 64);
    let mut b = SparseHyperLogLog::with_threshold(12, 4096);
    for i in 0..20 {
        a.add(&format!("a{i}"));
    }
    for i in 0..100 {
        b.add(&format!("b{i}"));
    }
    assert!(a.is_sparse());
    a.merge(&b).unwrap();
    assert!(!a.is_sparse(), "the combined list crosses 64 entries");
    let est = a.estimate();
    assert!(est > 100.0 && est < 140.0, "120 distinct, got {est}");
}

#[test]
fn merge_with_a_dense_peer_promotes_and_still_counts() {
    let mut a = SparseHyperLogLog::with_threshold(12, 4096);
    let mut b = SparseHyperLogLog::with_threshold(12, 8);
    for i in 0..50 {
        a.add(&format!("a{i}"));
    }
    for i in 0..500 {
        b.add(&format!("b{i}"));
    }
    assert!(a.is_sparse() && !b.is_sparse());
    a.merge(&b).unwrap();
    assert!(!a.is_sparse());
    let est = a.estimate();
    assert!(est > 490.0 && est < 620.0, "550 distinct, got {est}");
}

#[test]
fn merge_rejects_precision_mismatch() {
    let mut a = SparseHyperLogLog::new(12);
    let b = SparseHyperLogLog::new(10);
    assert_eq!(
        a.merge(&b).unwrap_err(),
        crate::HllError::PrecisionMismatch {
            left: 12,
            right: 10
        }
    );
}

#[test]
fn add_reports_whether_the_sketch_changed() {
    let mut h = SparseHyperLogLog::new(12);
    assert!(h.add("cpty-1"));
    assert!(!h.add("cpty-1"));
    h.promote();
    assert!(!h.add("cpty-1"), "still idempotent after promotion");
    assert!(h.add("cpty-2"));
}

#[test]
fn u64_and_string_paths_land_on_the_same_registers() {
    let mut a = SparseHyperLogLog::new(12);
    let mut b = SparseHyperLogLog::new(12);
    a.add_u64(42);
    b.add_bytes(&42u64.to_be_bytes());
    assert_eq!(a.to_dense().registers(), b.to_dense().registers());
}

#[test]
fn to_dense_bridges_into_the_set_ops_without_promoting() {
    let mut h = SparseHyperLogLog::new(12);
    for i in 0..40 {
        h.add(&format!("k{i}"));
    }
    let dense = h.to_dense();
    assert!(h.is_sparse(), "to_dense is non-destructive");
    let rel = (dense.estimate() - h.estimate()).abs() / h.estimate();
    assert!(rel < 1e-9, "the copy estimates identically");
}

#[test]
fn standard_error_matches_the_dense_envelope() {
    let h = SparseHyperLogLog::new(14);
    assert!((h.standard_error() - 1.04 / 16_384f64.sqrt()).abs() < 1e-12);
}
