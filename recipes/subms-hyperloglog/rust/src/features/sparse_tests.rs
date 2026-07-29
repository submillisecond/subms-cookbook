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
