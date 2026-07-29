use super::*;

#[test]
fn disjoint_sets_intersect_near_zero() {
    let mut a = HyperLogLog::new(12);
    let mut b = HyperLogLog::new(12);
    for i in 0..5_000 {
        a.add(&format!("a-{i}"));
        b.add(&format!("b-{i}"));
    }
    let inter = estimate_intersect(&a, &b).unwrap();
    // The HLL variance bound here is ~1.04/sqrt(4096) * 10_000 ~= 162.
    // Generous: under 5% of either source.
    assert!(inter < 500.0, "disjoint intersection ~0, got {inter}");
}

#[test]
fn identical_sets_intersect_near_cardinality() {
    let mut a = HyperLogLog::new(12);
    let mut b = HyperLogLog::new(12);
    for i in 0..5_000 {
        let k = format!("k-{i}");
        a.add(&k);
        b.add(&k);
    }
    let inter = estimate_intersect(&a, &b).unwrap();
    let rel = (inter - 5_000.0).abs() / 5_000.0;
    assert!(
        rel < 0.10,
        "identical intersection ~= |A|, got {inter} (rel {rel:.3})"
    );
}

#[test]
fn union_matches_merge() {
    let mut a = HyperLogLog::new(12);
    let mut b = HyperLogLog::new(12);
    for i in 0..3_000 {
        a.add(&format!("a-{i}"));
    }
    for i in 0..3_000 {
        b.add(&format!("b-{i}"));
    }
    let union = estimate_union(&a, &b).unwrap();
    let mut merged = HyperLogLog::new(12);
    merged.merge(&a).unwrap();
    merged.merge(&b).unwrap();
    let est = merged.estimate();
    let rel = (union - est).abs() / est.max(1.0);
    assert!(
        rel < 0.01,
        "union should equal merge: union={union} merged={est}"
    );
}

#[test]
fn partial_overlap_makes_sense() {
    let mut a = HyperLogLog::new(13);
    let mut b = HyperLogLog::new(13);
    // 10k items in A. 10k items in B. 3k items in both.
    for i in 0..10_000 {
        a.add(&format!("a-{i}"));
    }
    for i in 0..10_000 {
        b.add(&format!("b-{i}"));
    }
    for i in 0..3_000 {
        let k = format!("both-{i}");
        a.add(&k);
        b.add(&k);
    }
    let inter = estimate_intersect(&a, &b).unwrap();
    let rel = (inter - 3_000.0).abs() / 3_000.0;
    // Inclusion-exclusion noise: allow 50% relative error at
    // this scale. That's the well-known weakness; the test
    // documents it rather than ignoring it.
    assert!(rel < 0.5, "3k overlap, got {inter} (rel {rel:.3})");
}

#[test]
fn precision_mismatch_errors() {
    let a = HyperLogLog::new(12);
    let b = HyperLogLog::new(13);
    assert!(estimate_union(&a, &b).is_err());
    assert!(estimate_intersect(&a, &b).is_err());
}
