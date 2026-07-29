//! Pins the behaviour the `sample_app` example demonstrates: the base sketch
//! estimates a high-cardinality session stream within its error envelope, the
//! accuracy-vs-memory dial behaves, and each optional feature section holds.

use super::*;

#[test]
fn session_cardinality_scenario() {
    let true_distinct = 50_000u32;
    let mut hll = HyperLogLog::new(14);
    for i in 0..true_distinct {
        let sess = format!("sess-{i:08x}");
        for _ in 0..=(i % 5) {
            hll.add(&sess);
        }
    }
    let est = hll.estimate();
    let err = (est - f64::from(true_distinct)).abs() / f64::from(true_distinct);
    assert!(err < 0.05, "p=14 within 5%, got {err} (est {est})");
    assert_eq!(
        hll.register_count(),
        16_384,
        "p=14 is a 16 KB register array"
    );
}

#[test]
fn finer_precision_beats_the_five_percent_envelope() {
    let true_distinct = 100_000u32;
    let mut coarse = HyperLogLog::new(8);
    let mut fine = HyperLogLog::new(14);
    for i in 0..true_distinct {
        let k = format!("acct-{i:08x}");
        coarse.add(&k);
        fine.add(&k);
    }
    let fine_err = (fine.estimate() - f64::from(true_distinct)).abs() / f64::from(true_distinct);
    assert!(fine_err < 0.05, "p=14 within 5%, got {fine_err}");
    // The coarse sketch is 64x smaller and only has to stay inside its own
    // much wider envelope - the point of the tradeoff demo.
    let coarse_err =
        (coarse.estimate() - f64::from(true_distinct)).abs() / f64::from(true_distinct);
    assert!(
        coarse_err < 0.30,
        "p=8 within its wide envelope, got {coarse_err}"
    );
}

#[cfg(feature = "sparse")]
#[test]
fn sparse_stays_thin_then_promotes() {
    use crate::SparseHyperLogLog;
    let mut thin = SparseHyperLogLog::new(14);
    for cp in 0..20 {
        thin.add(&format!("cpty-{cp}"));
    }
    assert!(thin.is_sparse(), "thin name stays sparse");
    assert_eq!(
        thin.entry_count(),
        20,
        "one entry per distinct counterparty"
    );

    let mut hot = SparseHyperLogLog::new(8);
    for cp in 0..2_000 {
        hot.add(&format!("cpty-{cp}"));
    }
    assert!(!hot.is_sparse(), "busy name promotes to dense");
    let est = hot.estimate();
    assert!(
        est > 1_500.0 && est < 2_500.0,
        "hot estimate near 2000, got {est}"
    );
}

#[cfg(feature = "union-intersect")]
#[test]
fn union_and_overlap_across_venues() {
    use crate::{estimate_intersect, estimate_union};
    let mut venue_a = HyperLogLog::new(14);
    let mut venue_b = HyperLogLog::new(14);
    for i in 0..40_000 {
        venue_a.add(&format!("acct-{i}"));
    }
    for i in 20_000..60_000 {
        venue_b.add(&format!("acct-{i}"));
    }
    let union = estimate_union(&venue_a, &venue_b).unwrap();
    let inter = estimate_intersect(&venue_a, &venue_b).unwrap();
    assert!(
        (union - 60_000.0).abs() / 60_000.0 < 0.05,
        "union within 5%, got {union}"
    );
    assert!(
        (inter - 20_000.0).abs() / 20_000.0 < 0.25,
        "overlap within IE band, got {inter}"
    );
}
