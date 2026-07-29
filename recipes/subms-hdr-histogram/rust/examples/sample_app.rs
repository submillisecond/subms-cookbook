//! Sample app: a tour of `subms-hdr-histogram`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features merge`) to see the feature
//! sections light up.
//!
//! * base              - tick-to-trade latency capture with p50/p99/p999 reads
//!                       and Gil Tene coordinated-omission correction
//! * concurrent-writes - many feed-handler threads recording into one histogram
//! * dual-recorder     - lock-free interval percentile reporting
//! * merge             - roll per-shard histograms into a fleet-wide view
//! * decay             - recency-weighted percentiles that forget an old spike
//! * value-tagging     - slice latency by venue at query time
//! * iterators         - export the distribution as bands for a chart / sink

use subms_hdr_histogram::HdrHistogram;

fn main() {
    base_tick_to_trade();

    #[cfg(feature = "concurrent-writes")]
    concurrent_feed_handlers();

    #[cfg(feature = "dual-recorder")]
    dual_recorder_interval_report();

    #[cfg(feature = "merge")]
    merge_shard_rollup();

    #[cfg(feature = "decay")]
    decay_recency_weighted();

    #[cfg(feature = "value-tagging")]
    value_tagging_by_venue();

    #[cfg(feature = "iterators")]
    iterators_export_bands();
}

/// Base API: a strategy records tick-to-trade latencies (nanoseconds) into a
/// 3-significant-digit histogram, then reads p50/p99/p999. Recording is one
/// bucket increment; a percentile is a cumulative sweep over the counter array.
/// The second half shows why the sample source matters: under a fixed-rate
/// load, `record_with_expected_interval` backfills the requests a stall
/// blocked, so the tail reflects what the system delivered rather than the one
/// event that hurt.
fn base_tick_to_trade() {
    println!("== base: tick-to-trade latency capture ==");
    let mut h = HdrHistogram::new(3);

    // A right-skewed latency stream: most ops sit in a tight band, a small
    // fraction spike into the tail. Deterministic xorshift so the numbers are
    // reproducible.
    let mut rng = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let n = 2_000u64;
    for i in 0..n {
        let base = 700 + next() % 300; // 700..1000 ns steady state
        if i % 50 == 0 {
            h.record(4_000 + next() % 4_000); // ~2% tail spike, 4us..8us
        } else {
            h.record(base);
        }
    }

    let p50 = h.value_at_percentile(0.50);
    let p99 = h.value_at_percentile(0.99);
    let p999 = h.value_at_percentile(0.999);
    println!("  n={n} p50={p50}ns p99={p99}ns p999={p999}ns max={}ns", h.max());
    assert_eq!(h.count(), n, "every sample recorded");
    assert!(p50 <= 1_100, "median sits in the steady-state band: p50={p50}");
    assert!(p99 >= 2_000, "the 2% tail lifts p99 well past the median: p99={p99}");
    assert!(p999 >= p99 && h.max() >= p999, "percentiles are monotone");

    // Coordinated omission: a fixed-rate loop issues one op every 10 ns, then
    // stalls for 1000 ns. The naive histogram sees one slow sample; the
    // corrected one backfills the 99 requests the stall blocked.
    let mut naive = HdrHistogram::new(3);
    let mut corrected = HdrHistogram::new(3);
    for _ in 0..1_000 {
        naive.record(10);
        corrected.record_with_expected_interval(10, 10);
    }
    naive.record(1_000);
    corrected.record_with_expected_interval(1_000, 10);
    let naive_p99 = naive.value_at_percentile(0.99);
    let corrected_p99 = corrected.value_at_percentile(0.99);
    println!("  coordinated omission: naive p99={naive_p99}ns, corrected p99={corrected_p99}ns");
    assert!(naive_p99 <= 20, "uncorrected tail hides the stall: {naive_p99}");
    assert!(corrected_p99 >= 500, "correction lifts the tail: {corrected_p99}");
}

/// `concurrent-writes` feature: several market-data feed handlers record into
/// one histogram from different threads with no external lock - the only
/// contention is the per-bucket atomic increment.
#[cfg(feature = "concurrent-writes")]
fn concurrent_feed_handlers() {
    use std::sync::Arc;
    use std::thread;
    use subms_hdr_histogram::ConcurrentHdrHistogram;
    println!("\n== concurrent-writes: many feed handlers, one histogram ==");
    let h = Arc::new(ConcurrentHdrHistogram::new(3));
    let threads = 4;
    let per_thread = 50_000u64;
    let mut handles = vec![];
    for _ in 0..threads {
        let h = h.clone();
        handles.push(thread::spawn(move || {
            for i in 0..per_thread {
                h.record((i % 1_000) + 500);
            }
        }));
    }
    for j in handles {
        j.join().unwrap();
    }
    println!("  {} records lock-free, p99={}ns", h.count(), h.value_at_percentile(0.99));
    assert_eq!(h.count(), threads as u64 * per_thread, "no writes lost under contention");
}

/// `dual-recorder` feature: producers record continuously; a reporter thread
/// grabs an interval snapshot on a timer by rotating the active side and
/// draining the inactive one, never blocking the producers.
#[cfg(feature = "dual-recorder")]
fn dual_recorder_interval_report() {
    use subms_hdr_histogram::DualRecorder;
    println!("\n== dual-recorder: lock-free interval percentile report ==");
    let rec = DualRecorder::new(3);
    for v in 1..=500u64 {
        rec.record(v);
    }
    let interval = rec.get_interval_histogram();
    println!("  interval count={}, p99={}", interval.count(), interval.value_at_percentile(0.99));
    let next = rec.get_interval_histogram();
    assert_eq!(interval.count(), 500, "first interval captured every record");
    assert_eq!(next.count(), 0, "the next interval starts empty after the rotate");
}

/// `merge` feature: two shards each keep their own histogram; a periodic
/// roll-up sums one into the other for a fleet-wide percentile view. The merge
/// is exact - identical to recording every value into a single histogram.
#[cfg(feature = "merge")]
fn merge_shard_rollup() {
    use subms_hdr_histogram::merge;
    println!("\n== merge: roll per-shard histograms into a fleet view ==");
    let mut shard_a = HdrHistogram::new(3);
    let mut shard_b = HdrHistogram::new(3);
    for v in 1..=500u64 {
        shard_a.record(v);
    }
    for v in 501..=1_000u64 {
        shard_b.record(v);
    }
    merge(&mut shard_a, &shard_b).expect("identical shape merges");
    println!(
        "  fleet count={}, p50={}, p99={}",
        shard_a.count(),
        shard_a.value_at_percentile(0.5),
        shard_a.value_at_percentile(0.99)
    );
    assert_eq!(shard_a.count(), 1_000, "both shards folded in");
    assert!(shard_a.value_at_percentile(0.99) >= 900, "the high tail came from shard b");
}

/// `decay` feature: an exponentially-decaying histogram so the current p99
/// reflects recent activity. An old burst of slow ops fades over a few
/// half-lives, so a later burst of fast ops dominates the read.
#[cfg(feature = "decay")]
fn decay_recency_weighted() {
    use subms_hdr_histogram::{DecayingHdrHistogram, ManualClock};
    println!("\n== decay: recency-weighted p50 forgets an old spike ==");
    let clock = ManualClock::new();
    let halflife = 1_000_000_000u64; // 1 second
    let mut h = DecayingHdrHistogram::new(3, halflife, &clock);
    for _ in 0..1_000 {
        h.record(5_000); // an old burst of slow ops
    }
    clock.advance_ns(halflife * 4); // four half-lives pass
    for _ in 0..1_000 {
        h.record(800); // a recent burst of fast ops
    }
    let p50 = h.value_at_percentile(0.5);
    println!("  decayed count~{:.0}, p50={p50}ns", h.count());
    assert!(p50 < 2_000, "recent fast ops dominate the decayed distribution: p50={p50}");
}

/// `value-tagging` feature: one histogram, a 1-byte tag per recording, so
/// per-venue tails can be read separately at query time without standing up N
/// histograms.
#[cfg(feature = "value-tagging")]
fn value_tagging_by_venue() {
    use subms_hdr_histogram::TaggedHdrHistogram;
    println!("\n== value-tagging: slice latency by venue ==");
    const COLO: u8 = 0;
    const REMOTE: u8 = 1;
    let mut h = TaggedHdrHistogram::new(3);
    for v in 500..=1_000u64 {
        h.record(v, COLO); // a fast co-located venue
    }
    for v in 5_000..=6_000u64 {
        h.record(v, REMOTE); // a slow remote venue
    }
    let p99_colo = h.value_at_percentile_for_tag(0.99, COLO);
    let p99_remote = h.value_at_percentile_for_tag(0.99, REMOTE);
    println!("  colo p99={p99_colo}ns, remote p99={p99_remote}ns");
    assert!(p99_colo < p99_remote, "each venue's tail reads on its own");
}

/// `iterators` feature: walk the whole distribution rather than pull single
/// percentiles - here, the powers-of-two bands and quartile lower bounds a
/// chart or downstream sink would render.
#[cfg(feature = "iterators")]
fn iterators_export_bands() {
    println!("\n== iterators: export the distribution as bands ==");
    let mut h = HdrHistogram::new(3);
    for v in 1..=1_000u64 {
        h.record(v);
    }
    let bands = h.iter_logarithmic().count();
    let quartiles: Vec<u64> = h.iter_percentiles(25.0).map(|e| e.value_lo).collect();
    println!("  {bands} log2 bands; quartile lower bounds = {quartiles:?}");
    assert!(bands > 0, "the populated range spans at least one band");
    assert!(!quartiles.is_empty(), "the percentile walk yields quartile buckets");
}
