//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base tick-to-trade capture, the coordinated-omission correction, and
//! each optional feature (gated the same way as the sample). Colocated with the
//! crate root and included via `#[path]` (see `lib.rs`).

use super::*;

#[test]
fn tick_to_trade_percentiles_are_monotone() {
    let mut h = HdrHistogram::new(3);
    let mut rng = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let n = 2_000u64;
    for i in 0..n {
        if i % 50 == 0 {
            h.record(4_000 + next() % 4_000);
        } else {
            h.record(700 + next() % 300);
        }
    }
    assert_eq!(h.count(), n);
    let p50 = h.value_at_percentile(0.50);
    let p99 = h.value_at_percentile(0.99);
    let p999 = h.value_at_percentile(0.999);
    assert!(p50 <= 1_100, "median in the steady band: {p50}");
    assert!(p99 >= 2_000, "the tail lifts p99: {p99}");
    assert!(p999 >= p99 && h.max() >= p999, "monotone tail");

    assert!(h.min() > 0, "the floor is a real recorded value");
    assert!(
        h.mean() >= p50 as f64,
        "the tail drags the mean above the median"
    );
    assert!(
        h.percentile_at_or_below_value(2_000) > 0.9,
        "most ops sit inside the 2us band"
    );
    assert_eq!(h.footprint_bytes() % 8, 0, "the array is u64 counters");
}

#[test]
fn coordinated_omission_lifts_the_tail() {
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
    assert!(
        naive_p99 <= 20,
        "uncorrected tail hides the stall: {naive_p99}"
    );
    assert!(
        corrected_p99 >= 500,
        "correction backfills the blocked requests: {corrected_p99}"
    );
    assert!(
        corrected_p99 > naive_p99,
        "corrected tail is strictly higher"
    );
}

#[cfg(feature = "concurrent-writes")]
#[test]
fn concurrent_writers_lose_nothing() {
    use crate::ConcurrentHdrHistogram;
    use std::sync::Arc;
    use std::thread;
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
    assert_eq!(h.count(), threads as u64 * per_thread);
}

#[cfg(feature = "dual-recorder")]
#[test]
fn dual_recorder_interval_then_empty() {
    use crate::DualRecorder;
    let rec = DualRecorder::new(3);
    for v in 1..=500u64 {
        rec.record(v);
    }
    let interval = rec.get_interval_histogram();
    let next = rec.get_interval_histogram();
    assert_eq!(interval.count(), 500);
    assert_eq!(next.count(), 0);
}

#[cfg(feature = "merge")]
#[test]
fn merge_rolls_shards_into_fleet() {
    use crate::merge;
    let mut shard_a = HdrHistogram::new(3);
    let mut shard_b = HdrHistogram::new(3);
    for v in 1..=500u64 {
        shard_a.record(v);
    }
    for v in 501..=1_000u64 {
        shard_b.record(v);
    }
    merge(&mut shard_a, &shard_b).unwrap();
    assert_eq!(shard_a.count(), 1_000);
    assert!(shard_a.value_at_percentile(0.99) >= 900);
}

#[cfg(feature = "decay")]
#[test]
fn decay_weights_recent_activity() {
    use crate::{DecayingHdrHistogram, ManualClock};
    let clock = ManualClock::new();
    let halflife = 1_000_000_000u64;
    let mut h = DecayingHdrHistogram::new(3, halflife, &clock);
    for _ in 0..1_000 {
        h.record(5_000);
    }
    clock.advance_ns(halflife * 4);
    for _ in 0..1_000 {
        h.record(800);
    }
    assert!(
        h.value_at_percentile(0.5) < 2_000,
        "recent fast ops dominate"
    );
}

#[cfg(feature = "value-tagging")]
#[test]
fn value_tagging_separates_venues() {
    use crate::TaggedHdrHistogram;
    const COLO: u8 = 0;
    const REMOTE: u8 = 1;
    let mut h = TaggedHdrHistogram::new(3);
    for v in 500..=1_000u64 {
        h.record(v, COLO);
    }
    for v in 5_000..=6_000u64 {
        h.record(v, REMOTE);
    }
    assert!(
        h.value_at_percentile_for_tag(0.99, COLO) < h.value_at_percentile_for_tag(0.99, REMOTE)
    );
}

#[cfg(feature = "iterators")]
#[test]
fn iterators_export_bands_and_quartiles() {
    let mut h = HdrHistogram::new(3);
    for v in 1..=1_000u64 {
        h.record(v);
    }
    assert!(h.iter_logarithmic().count() > 0);
    let quartiles: Vec<u64> = h.iter_percentiles(25.0).map(|e| e.value_lo).collect();
    assert!(!quartiles.is_empty());
}
