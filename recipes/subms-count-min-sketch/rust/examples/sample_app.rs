//! Sample app: a per-symbol message-rate governor for a market-data gateway.
//!
//! The gateway sees a high-cardinality tape - a handful of hot futures plus a
//! long tail of thinly-traded symbols - and has to answer three questions in
//! fixed memory: how fast is this symbol talking, which symbols are the loudest,
//! and has anything burst in the last few seconds. An exact per-symbol counter
//! answers all three and grows with the symbol universe, which is the thing the
//! gateway cannot afford.
//!
//! Run the base with `cargo run --example sample_app`; add `--all-features`
//! (or a subset like `--features windowed`) to light up the feature sections.
//!
//! * base          - the governor itself: sizing, rate verdicts, checkpointing
//! * heavy-hitters - the throttle list, ranked by bytes rather than messages
//! * windowed      - burst detection that ages out
//! * merge         - folding per-venue shards at the join

use std::collections::HashMap;

use subms_count_min_sketch::CountMinSketch;

fn main() {
    let tape = tape();
    let gov = RateGovernor::ingest(&tape);
    gov.report(&tape);
    gov.checkpoint();

    #[cfg(feature = "heavy-hitters")]
    throttle_list(&tape);

    #[cfg(feature = "windowed")]
    burst_governor();

    #[cfg(feature = "merge")]
    cross_venue_rollup();
}

/// One tape entry: a symbol and the size of the message that carried it.
struct Msg {
    symbol: String,
    bytes: u32,
}

/// A deterministic replay of one gateway second. Four hot futures dominate the
/// message count; the 4000 thin symbols behind them are what an exact counter
/// would have to size for.
fn tape() -> Vec<Msg> {
    let mut tape = Vec::new();
    for (symbol, msgs, bytes) in [
        ("ESZ5", 5000, 96),
        ("NQZ5", 3000, 96),
        ("CLF6", 1500, 128),
        ("ZNH6", 900, 64),
    ] {
        for _ in 0..msgs {
            tape.push(Msg {
                symbol: symbol.to_string(),
                bytes,
            });
        }
    }
    for i in 0..4000 {
        tape.push(Msg {
            symbol: format!("THIN{i:04}"),
            bytes: 64,
        });
    }
    tape
}

/// The governor: one sketch, one threshold, one verdict per symbol.
struct RateGovernor {
    rates: CountMinSketch,
    limit: u32,
}

enum Verdict {
    Pass,
    Throttle,
}

impl RateGovernor {
    /// Size from the error budget rather than from a guessed (d, w): tolerate
    /// an over-count of 0.1% of gateway volume, 99.9% of the time.
    fn ingest(tape: &[Msg]) -> Self {
        let mut rates = CountMinSketch::with_error_bounds(0.001, 0.999);
        for msg in tape {
            rates.add(&msg.symbol);
        }
        Self {
            rates,
            limit: 2_000,
        }
    }

    /// The estimate is an upper bound, so a symbol under the limit is under it
    /// for certain. That is the direction a governor wants to be wrong in.
    fn verdict(&self, symbol: &str) -> Verdict {
        if self.rates.estimate(symbol) > self.limit {
            Verdict::Throttle
        } else {
            Verdict::Pass
        }
    }

    fn report(&self, tape: &[Msg]) {
        println!("== governor: per-symbol message rates ==");
        let mut exact: HashMap<&str, u32> = HashMap::new();
        for msg in tape {
            *exact.entry(msg.symbol.as_str()).or_insert(0) += 1;
        }
        println!(
            "  {} messages, {} distinct symbols",
            tape.len(),
            exact.len()
        );
        println!(
            "  sketch {}x{} = {} KiB, error <= {:.4}% of volume at {:.3} confidence",
            self.rates.depth(),
            self.rates.width(),
            self.rates.heap_bytes() / 1024,
            self.rates.relative_error() * 100.0,
            self.rates.confidence()
        );
        println!(
            "  volume {}, over-count budget {} msgs, cells touched {:.1}%",
            self.rates.total(),
            self.rates.error_margin(),
            self.rates.occupancy() * 100.0
        );

        let mut worst_over = 0u32;
        for (symbol, &truth) in &exact {
            let est = self.rates.estimate(symbol);
            assert!(
                est >= truth,
                "estimate must never fall below the true count"
            );
            worst_over = worst_over.max(est - truth);
        }
        println!("  worst over-count across every symbol: {worst_over}");

        for symbol in ["ESZ5", "NQZ5", "CLF6", "ZNH6", "THIN0007"] {
            let est = self.rates.estimate(symbol);
            let lo = self.rates.estimate_lower_bound(symbol);
            let verdict = match self.verdict(symbol) {
                Verdict::Throttle => "THROTTLE",
                Verdict::Pass => "pass",
            };
            println!(
                "  {symbol:<9} {lo}..{est} msgs (true {}) -> {verdict}",
                exact[symbol]
            );
        }
    }

    /// The gateway restarts often. A snapshot is a plain byte buffer, so the
    /// governor comes back with its rate history instead of a cold sketch.
    fn checkpoint(&self) {
        println!("\n== checkpoint: survive a gateway restart ==");
        let bytes = self.rates.to_bytes();
        let restored = CountMinSketch::from_bytes(&bytes).expect("own snapshot decodes");
        println!(
            "  {} bytes on the wire; restored {}x{}, volume {}",
            bytes.len(),
            restored.depth(),
            restored.width(),
            restored.total()
        );
        println!(
            "  ESZ5 before {} / after {}",
            self.rates.estimate("ESZ5"),
            restored.estimate("ESZ5")
        );
        assert_eq!(restored.estimate("ESZ5"), self.rates.estimate("ESZ5"));
    }
}

/// `heavy-hitters`: the base sketch scores a symbol you name, but it cannot
/// list the loudest symbols on its own - the cell layout is lossy by design.
/// The throttle list needs the ranking, and it ranks on BYTES rather than
/// messages, because a 128-byte depth update costs the gateway twice what a
/// 64-byte trade print does. Weighted add is the same call with the size.
#[cfg(feature = "heavy-hitters")]
fn throttle_list(tape: &[Msg]) {
    use subms_count_min_sketch::HeavyHitters;
    println!("\n== throttle list: loudest symbols by bandwidth ==");
    let mut by_bytes = HeavyHitters::new(3, 5, 8192);
    let mut by_msgs = HeavyHitters::new(3, 5, 8192);
    for msg in tape {
        by_bytes.add_n(&msg.symbol, msg.bytes);
        by_msgs.add(&msg.symbol);
    }
    println!(
        "  {} bytes ranked, top {} held:",
        by_bytes.total(),
        by_bytes.k()
    );
    for entry in by_bytes.top() {
        println!("    {:<9} ~{} bytes", entry.key, entry.estimate);
    }
    let top = by_bytes.top();
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].key, "ESZ5");

    // CLF6 carries 128-byte depth updates against ZNH6's 64-byte prints, so
    // weighting by size roughly doubles the gap the message count reports.
    let (bytes_gap, msg_gap) = (
        by_bytes.estimate("CLF6") as f64 / by_bytes.estimate("ZNH6") as f64,
        by_msgs.estimate("CLF6") as f64 / by_msgs.estimate("ZNH6") as f64,
    );
    println!("  CLF6 over ZNH6: {bytes_gap:.2}x by bytes, {msg_gap:.2}x by messages");
}

/// `windowed`: an all-time counter never forgets, so a burst that ended a
/// minute ago still trips the limit. A ring of sub-sketches ages it out - the
/// caller owns the clock by choosing when to tick, one tick per second here.
#[cfg(feature = "windowed")]
fn burst_governor() {
    use subms_count_min_sketch::WindowedCountMinSketch;
    println!("\n== burst governor: a 3-second window ==");
    let mut recent = WindowedCountMinSketch::new(3, 5, 8192);
    let limit = 400;

    for _ in 0..500 {
        recent.add("ESZ5");
    }
    println!(
        "  after a 500-message burst: in-window {} (limit {limit}) -> {}",
        recent.estimate("ESZ5"),
        if recent.estimate("ESZ5") > limit {
            "THROTTLE"
        } else {
            "pass"
        }
    );

    for second in 1..=3 {
        recent.tick();
        for _ in 0..40 {
            recent.add("ESZ5");
        }
        println!(
            "  +{second}s quiet trading: in-window {}",
            recent.estimate("ESZ5")
        );
    }
    let settled = recent.estimate("ESZ5");
    println!("  burst aged out, {settled} msgs in window -> pass");
    assert!(settled <= limit, "the burst left the window");
    println!(
        "  window costs {} KiB, {}x a single sketch",
        recent.heap_bytes() / 1024,
        recent.slices()
    );
}

/// `merge`: the gateway runs one handler per venue, each with its own sketch
/// and no shared state on the write path. The rollup happens once, at the join.
/// Cells are summed because a symbol trades on both venues, and summing is what
/// keeps the merged estimate above the true cross-venue count.
#[cfg(feature = "merge")]
fn cross_venue_rollup() {
    use subms_count_min_sketch::merge_into;
    println!("\n== rollup: two venue shards folded at the join ==");
    let mut cme = CountMinSketch::new(5, 8192);
    let mut ice = CountMinSketch::new(5, 8192);
    for _ in 0..300 {
        cme.add("ESZ5");
    }
    for _ in 0..120 {
        cme.add("CLF6");
    }
    for _ in 0..200 {
        ice.add("CLF6");
    }

    merge_into(&mut cme, &ice).expect("same shape and seed");
    println!("  ESZ5 (CME only):     {}", cme.estimate("ESZ5"));
    println!(
        "  CLF6 (both venues):  {} (120 + 200)",
        cme.estimate("CLF6")
    );
    println!("  rolled-up volume:    {}", cme.total());
    assert!(cme.estimate("ESZ5") >= 300);
    assert!(
        cme.estimate("CLF6") >= 320,
        "the union count, not the larger leg"
    );
}
