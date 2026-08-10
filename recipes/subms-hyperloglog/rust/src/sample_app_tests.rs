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

#[test]
fn collector_fan_in_over_the_wire() {
    // Two venue sketches, shipped as bytes and merged by a collector that
    // never sees an account id - the sample app's closing stage.
    let mut venue_a = HyperLogLog::new(14);
    let mut venue_b = HyperLogLog::new(14);
    for i in 0..30_000u64 {
        venue_a.add_u64(i);
    }
    for i in 20_000..50_000u64 {
        venue_b.add_u64(i);
    }
    let shipped: Vec<Vec<u8>> = vec![venue_a.to_bytes(), venue_b.to_bytes()];
    assert!(
        shipped.iter().all(|b| b.len() == 8 + 16_384),
        "a p=14 sketch is 16392 bytes on the wire whatever it counted"
    );

    let mut firm = HyperLogLog::new(14);
    for bytes in &shipped {
        firm.merge(&HyperLogLog::from_bytes(bytes).unwrap())
            .unwrap();
    }
    let est = firm.estimate();
    assert!(
        (est - 50_000.0).abs() / 50_000.0 < 0.05,
        "50k distinct accounts firm-wide, got {est}"
    );
}

#[cfg(feature = "sparse")]
#[test]
fn thin_symbol_sketches_stay_thin_on_the_wire() {
    use crate::SparseHyperLogLog;
    let mut thin = SparseHyperLogLog::with_threshold(14, 2_000);
    for cp in 0..30u64 {
        thin.add_u64(cp);
    }
    let bytes = thin.to_bytes();
    assert!(
        bytes.len() < 200,
        "30 counterparties should not cost 16 KB on the wire, got {}",
        bytes.len()
    );
    let back = SparseHyperLogLog::from_bytes(&bytes).unwrap();
    assert_eq!(back.estimate(), thin.estimate());
}
