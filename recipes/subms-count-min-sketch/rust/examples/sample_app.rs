//! Sample app: a tour of `subms-count-min-sketch` over a market-data feed,
//! base API first, then each optional feature. Run the base with
//! `cargo run --example sample_app`; add `--all-features` (or a subset like
//! `--features heavy-hitters`) to light up the feature sections.
//!
//! Scenario: a high-cardinality tape of per-symbol market-data messages,
//! where the hot symbols have to be found in fixed memory - no exact
//! per-symbol counter that grows with the symbol universe.
//!
//! * base          - per-symbol message-rate estimate, overestimate-only error
//! * heavy-hitters - the hottest symbols themselves (hot-symbol detection)
//! * windowed      - a recent-rate estimate that ages bursts out by slice
//! * merge         - fan-in of per-shard sketches from sharded feed handlers

use std::collections::HashMap;

use subms_count_min_sketch::CountMinSketch;

fn main() {
    base_symbol_rate();

    #[cfg(feature = "heavy-hitters")]
    hot_symbol_detection();

    #[cfg(feature = "windowed")]
    recent_rate_window();

    #[cfg(feature = "merge")]
    fan_in_merge();
}

/// A synthetic tape: four hot symbols dominate, plus a long tail of cold
/// symbols each seen once. The long tail is the high-cardinality part an
/// exact `HashMap` counter would have to size for.
fn market_stream() -> Vec<String> {
    let mut stream = Vec::new();
    for (sym, hits) in [("ES", 5000), ("NQ", 3000), ("CL", 1500), ("ZN", 900)] {
        for _ in 0..hits {
            stream.push(sym.to_string());
        }
    }
    for i in 0..4000 {
        stream.push(format!("T{i:04}"));
    }
    stream
}

/// Base API: estimate how many messages each symbol sent, in fixed memory.
/// The Count-Min guarantee is one-sided - the estimate is always >= the
/// true count, never below - so a rate threshold built on it never misses
/// a genuinely hot symbol; the only error is a bounded over-count.
fn base_symbol_rate() {
    println!("== base: per-symbol message-rate estimate ==");
    let stream = market_stream();

    let mut exact: HashMap<String, u32> = HashMap::new();
    let mut cms = CountMinSketch::new(4, 4096);
    for sym in &stream {
        *exact.entry(sym.clone()).or_insert(0) += 1;
        cms.add(sym);
    }
    println!(
        "  {} messages, {} distinct symbols, into a {}x{} sketch",
        stream.len(),
        exact.len(),
        cms.depth(),
        cms.width()
    );

    let mut worst_over = 0u32;
    for (sym, &truth) in &exact {
        let est = cms.estimate(sym);
        assert!(est >= truth, "estimate must never underestimate");
        worst_over = worst_over.max(est - truth);
    }
    println!(
        "  estimate >= true for all {} symbols; worst over-count {worst_over}",
        exact.len()
    );

    for sym in ["ES", "NQ", "CL", "ZN"] {
        println!("  {sym}: est {} (true {})", cms.estimate(sym), exact[sym]);
    }
    let cold = "T0007";
    println!(
        "  cold {cold}: est {} (true {})",
        cms.estimate(cold),
        exact[cold]
    );
    assert!(
        cms.estimate(cold) >= exact[cold],
        "cold key not under-counted"
    );
}

/// `heavy-hitters` feature: the base sketch scores a symbol you name, but
/// it cannot list the hottest symbols on its own - the cell layout is lossy
/// by design. `HeavyHitters` keeps a top-K side index refreshed on every
/// add, so the hot symbols fall out directly.
#[cfg(feature = "heavy-hitters")]
fn hot_symbol_detection() {
    use subms_count_min_sketch::HeavyHitters;
    println!("\n== heavy-hitters: the hottest symbols ==");
    let mut hh = HeavyHitters::new(3, 4, 4096);
    for sym in market_stream() {
        hh.add(&sym);
    }
    println!("  top {} symbols:", hh.k());
    for entry in hh.top() {
        println!("    {} ~{}", entry.key, entry.estimate);
    }
    let top = hh.top();
    assert_eq!(top.len(), 3, "exactly K tracked");
    assert_eq!(top[0].key, "ES", "the busiest symbol leads the board");
    assert_eq!(top[1].key, "NQ");
    assert_eq!(top[2].key, "CL");
}

/// `windowed` feature: an all-time counter never forgets. A ring of
/// sub-sketches ages old bursts out - `tick` rotates the ring and clears
/// the slice that just rolled over, so the estimate reflects only the
/// recent window. The caller owns the clock by choosing when to tick.
#[cfg(feature = "windowed")]
fn recent_rate_window() {
    use subms_count_min_sketch::WindowedCountMinSketch;
    println!("\n== windowed: recent message rate ==");
    let mut w = WindowedCountMinSketch::new(3, 4, 4096);
    for _ in 0..500 {
        w.add("ES");
    }
    println!("  ES in-window right after the burst: {}", w.estimate("ES"));
    assert!(w.estimate("ES") >= 500);

    w.tick();
    w.tick();
    w.tick();
    println!(
        "  ES in-window after the slice rotated out: {}",
        w.estimate("ES")
    );
    assert_eq!(w.estimate("ES"), 0, "the burst aged out of the window");
}

/// `merge` feature: shard the tape across feed handlers, each accumulating
/// its own lock-free sketch of identical shape, then fold at the join. The
/// combiner is element-wise MAX, not sum - each shard already absorbed its
/// own conservative-update damping, so pointwise addition would double-count
/// a symbol that traded on more than one venue.
#[cfg(feature = "merge")]
fn fan_in_merge() {
    use subms_count_min_sketch::merge_into;
    println!("\n== merge: fan-in of per-shard sketches ==");
    let mut venue_a = CountMinSketch::new(4, 4096);
    let mut venue_b = CountMinSketch::new(4, 4096);
    for _ in 0..300 {
        venue_a.add("ES");
    }
    for _ in 0..120 {
        venue_a.add("NQ");
    }
    for _ in 0..200 {
        venue_b.add("NQ");
    }

    merge_into(&mut venue_a, &venue_b).expect("identical shape merges");
    let es = venue_a.estimate("ES");
    let nq = venue_a.estimate("NQ");
    println!("  merged ES: {es}");
    println!("  merged NQ: {nq}  (max of 120 and 200, not their sum)");
    assert!(es >= 300, "ES only traded on venue A");
    assert!(
        (200..320).contains(&nq),
        "max-merge keeps NQ near 200, not 320"
    );
}
