//! Sample app: a tour of `subms-hyperloglog`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features sparse`) to see the feature
//! sections light up.
//!
//! * base - distinct client-session count in a trading gateway, plus the
//!   accuracy-vs-memory tradeoff across precisions
//! * sparse - one distinct-counterparty sketch per instrument, kept cheap for
//!   the thin long tail until a name gets busy
//! * union-intersect - distinct account reach and overlap across two venues,
//!   without shipping raw id lists between them

use subms_hyperloglog::HyperLogLog;

fn main() {
    base_session_cardinality();
    accuracy_vs_memory();

    #[cfg(feature = "sparse")]
    sparse_per_instrument();

    #[cfg(feature = "union-intersect")]
    cross_venue_reach();
}

/// Base API: a gateway sees a firehose of order messages that share client
/// sessions. We want the distinct active-session count per window without
/// holding every id in an unbounded `HashSet`. HLL answers it from a fixed
/// 16 KB register array; duplicates in the stream are idempotent.
fn base_session_cardinality() {
    println!("== base: distinct client sessions in a trading gateway ==");
    let true_distinct = 50_000u32;
    let mut hll = HyperLogLog::new(14);
    let mut events = 0u64;
    for i in 0..true_distinct {
        let sess = format!("sess-{i:08x}");
        // Each session sends a short burst of messages; only the distinct
        // session count moves the estimate.
        for _ in 0..=(i % 5) {
            hll.add(&sess);
            events += 1;
        }
    }
    let est = hll.estimate();
    let err = (est - f64::from(true_distinct)).abs() / f64::from(true_distinct);
    println!("  stream:   {events} messages, {true_distinct} distinct sessions");
    println!("  estimate: {est:.0}  ({:.2}% error)", err * 100.0);
    println!(
        "  state:    {} registers = ~{} KB, fixed no matter the stream length",
        hll.register_count(),
        hll.register_count() / 1024
    );
    assert!(err < 0.05, "p=14 estimate within 5%, got {err}");
}

/// Base API: the accuracy-vs-memory dial. Precision `p` fixes the register
/// count `m = 2^p` and therefore the byte budget; the standard error falls out
/// as ~1.04/sqrt(m). Smaller state, larger error - the same distinct stream
/// estimated at three precisions makes the trade explicit.
fn accuracy_vs_memory() {
    println!("\n== base: the accuracy-vs-memory tradeoff ==");
    let true_distinct = 100_000u32;
    let mut prev_err = f64::INFINITY;
    for p in [8u32, 11, 14] {
        let mut hll = HyperLogLog::new(p);
        for i in 0..true_distinct {
            hll.add(&format!("acct-{i:08x}"));
        }
        let est = hll.estimate();
        let err = (est - f64::from(true_distinct)).abs() / f64::from(true_distinct);
        let kb = f64::from(hll.register_count()) / 1024.0;
        println!(
            "  p={p:<2} m={:<6} ~{kb:>5.1} KB  estimate {est:>9.0}  err {:.2}%",
            hll.register_count(),
            err * 100.0
        );
        prev_err = prev_err.min(err);
    }
    // The finest precision must comfortably clear its ~0.8% envelope; the
    // running min guards against a fluke run at the coarse end being trusted.
    assert!(
        prev_err < 0.05,
        "best precision estimate within 5%, got {prev_err}"
    );
}

/// `sparse` feature: a risk system keeps one distinct-counterparty sketch per
/// instrument. Most instruments are thinly traded, so a dense 16 KB array each
/// would waste memory across the whole book. `SparseHyperLogLog` holds a
/// compact `(index, rho)` list while cardinality is low and promotes to the
/// dense array only once a name gets busy enough to justify it.
#[cfg(feature = "sparse")]
fn sparse_per_instrument() {
    use subms_hyperloglog::SparseHyperLogLog;
    println!("\n== sparse: distinct counterparties per instrument (long tail) ==");

    let mut thin = SparseHyperLogLog::new(14);
    for cp in 0..20 {
        thin.add(&format!("cpty-{cp}"));
    }
    println!(
        "  thin name: 20 counterparties -> sparse={}, {} entries held (no 16 KB array)",
        thin.is_sparse(),
        thin.entry_count()
    );
    assert!(thin.is_sparse(), "a thinly-traded instrument stays sparse");

    // A small precision here so the demo crosses the promotion threshold
    // quickly; the effect is identical at p=14 with more counterparties.
    let mut hot = SparseHyperLogLog::new(8);
    for cp in 0..2_000 {
        hot.add(&format!("cpty-{cp}"));
    }
    let est = hot.estimate();
    println!(
        "  hot name:  2000 counterparties -> promoted to dense={}, estimate {est:.0}",
        !hot.is_sparse()
    );
    assert!(!hot.is_sparse(), "a busy instrument promotes to dense");
    assert!(
        est > 1_500.0 && est < 2_500.0,
        "hot estimate near 2000, got {est}"
    );
}

/// `union-intersect` feature: two venues each keep an active-account sketch.
/// Merging them estimates the total distinct reach; inclusion-exclusion
/// estimates the accounts trading on both. Only the 16 KB sketches move
/// between venues, never the raw account ids.
#[cfg(feature = "union-intersect")]
fn cross_venue_reach() {
    use subms_hyperloglog::{estimate_intersect, estimate_union};
    println!("\n== union-intersect: distinct accounts across two venues ==");

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
    println!("  venue A: 40k accounts, venue B: 40k accounts, 20k shared");
    println!("  total reach (union):   {union:>7.0}  (true 60000)");
    println!("  both venues (overlap): {inter:>7.0}  (true 20000)");
    assert!(
        (union - 60_000.0).abs() / 60_000.0 < 0.05,
        "union within 5%, got {union}"
    );
    // Inclusion-exclusion error scales with |A| + |B|, not the overlap, so
    // this band is wider than the union's on purpose.
    assert!(
        (inter - 20_000.0).abs() / 20_000.0 < 0.25,
        "overlap within the IE noise band, got {inter}"
    );
}
