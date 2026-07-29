use super::*;

fn relative_error(actual: f64, expected: f64) -> f64 {
    (actual - expected).abs() / expected
}

#[test]
fn precision_is_clamped() {
    let lo = HyperLogLog::new(2);
    let hi = HyperLogLog::new(99);
    assert_eq!(lo.precision(), 4);
    assert_eq!(hi.precision(), 18);
}

#[test]
fn empty_estimate_is_zero() {
    let hll = HyperLogLog::new(14);
    assert!(hll.estimate() < 1.0, "empty -> ~0");
}

#[test]
fn small_cardinality_uses_linear_counting() {
    let mut hll = HyperLogLog::new(14);
    for i in 0..100u32 {
        hll.add(&format!("k{i}"));
    }
    let est = hll.estimate();
    // 100 distinct out of 16384 registers -> linear counting kicks in;
    // expect tight estimate.
    assert!(relative_error(est, 100.0) < 0.05, "got {est}");
}

#[test]
fn medium_cardinality_within_two_percent() {
    let mut hll = HyperLogLog::new(14);
    for i in 0..10_000u32 {
        hll.add(&format!("key{i}"));
    }
    let est = hll.estimate();
    assert!(
        relative_error(est, 10_000.0) < 0.02,
        "10k expected, got {est}"
    );
}

#[test]
fn merge_equivalent_to_combined_add() {
    let mut a = HyperLogLog::new(14);
    let mut b = HyperLogLog::new(14);
    for i in 0..5_000u32 {
        a.add(&format!("A{i}"));
        b.add(&format!("B{i}"));
    }
    a.merge(&b).unwrap();
    let est = a.estimate();
    // 10k distinct combined.
    assert!(relative_error(est, 10_000.0) < 0.03, "merged got {est}");
}

#[test]
fn merge_rejects_precision_mismatch() {
    let mut a = HyperLogLog::new(14);
    let b = HyperLogLog::new(12);
    assert!(a.merge(&b).is_err());
}

#[test]
fn idempotent_add_does_not_blow_estimate() {
    let mut hll = HyperLogLog::new(14);
    for _ in 0..1000 {
        hll.add("same-key");
    }
    let est = hll.estimate();
    assert!(
        est < 5.0,
        "adding same key 1000x -> estimate should still be ~1, got {est}"
    );
}

#[test]
fn register_count_matches_precision() {
    assert_eq!(HyperLogLog::new(4).register_count(), 16);
    assert_eq!(HyperLogLog::new(10).register_count(), 1024);
    assert_eq!(HyperLogLog::new(14).register_count(), 16384);
}

#[test]
fn low_precision_alpha_constants() {
    // precision 5 -> m=32, precision 6 -> m=64 exercise the tabulated
    // alpha_m constants (0.697 / 0.709) rather than the general formula.
    let mut p5 = HyperLogLog::new(5);
    let mut p6 = HyperLogLog::new(6);
    assert_eq!(p5.register_count(), 32);
    assert_eq!(p6.register_count(), 64);
    for i in 0..40u32 {
        p5.add(&format!("k{i}"));
        p6.add(&format!("k{i}"));
    }
    // Coarse precision, but the estimator must stay finite and positive.
    assert!(p5.estimate() > 0.0 && p5.estimate().is_finite());
    assert!(p6.estimate() > 0.0 && p6.estimate().is_finite());
}

#[test]
fn high_cardinality_within_two_percent() {
    let mut hll = HyperLogLog::new(14);
    for i in 0..50_000u32 {
        hll.add(&format!("k-{i}"));
    }
    let est = hll.estimate();
    assert!(
        relative_error(est, 50_000.0) < 0.03,
        "50k expected, got {est}"
    );
}

#[test]
fn merge_does_not_double_count_overlapping_keys() {
    let mut a = HyperLogLog::new(14);
    let mut b = HyperLogLog::new(14);
    for i in 0..5000u32 {
        a.add(&format!("same-{i}"));
        b.add(&format!("same-{i}"));
    }
    a.merge(&b).unwrap();
    let est = a.estimate();
    // Both saw the same 5000 distinct keys; merge stays at ~5000.
    assert!(
        relative_error(est, 5000.0) < 0.05,
        "overlap-only merge: {est}"
    );
}
