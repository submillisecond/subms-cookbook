//! Unit tests for the base `HdrHistogram`. Colocated with the module and
//! included via `#[path]` (see `lib.rs`), so they live alongside the code and
//! can reach internals if needed.

use super::*;

#[test]
fn empty_returns_zero() {
    let h = HdrHistogram::new(3);
    assert_eq!(h.count(), 0);
    assert_eq!(h.max(), 0);
    assert_eq!(h.value_at_percentile(0.99), 0);
}

#[test]
fn records_and_counts() {
    let mut h = HdrHistogram::new(3);
    for v in [10u64, 20, 30, 40, 50] {
        h.record(v);
    }
    assert_eq!(h.count(), 5);
    assert!(h.max() >= 50);
}

#[test]
fn percentiles_match_distribution() {
    let mut h = HdrHistogram::new(3);
    for i in 1..=1000u64 {
        h.record(i);
    }
    let p50 = h.value_at_percentile(0.50);
    let p99 = h.value_at_percentile(0.99);
    assert!((450..=550).contains(&p50), "p50={p50}");
    assert!((950..=1050).contains(&p99), "p99={p99}");
}

#[test]
fn handles_large_values() {
    let mut h = HdrHistogram::new(3);
    let big = 1_000_000_000u64;
    // Mix: 99 small, 1 large. p99 should land in the large bucket.
    for _ in 0..99 {
        h.record(10);
    }
    h.record(big);
    assert_eq!(h.count(), 100);
    // max() returns the bucket lower bound; within significant-digit precision.
    assert!(
        h.max() as f64 >= big as f64 * 0.99,
        "max {} too low for {big}",
        h.max()
    );
    assert!(h.value_at_percentile(1.0) as f64 >= big as f64 * 0.99);
}

#[test]
fn precision_is_clamped() {
    let h = HdrHistogram::new(0);
    assert!(h.sub_count() >= 2);
    let h = HdrHistogram::new(99);
    assert!(h.sub_count() > 0);
}

#[test]
fn single_value_recorded() {
    let mut h = HdrHistogram::new(3);
    h.record(123);
    assert_eq!(h.count(), 1);
    assert!(h.max() >= 123);
    assert!(h.value_at_percentile(0.5) >= 100);
}

#[test]
fn percentile_zero_returns_minimum() {
    let mut h = HdrHistogram::new(3);
    for i in 1..=100u64 {
        h.record(i);
    }
    let p0 = h.value_at_percentile(0.0);
    assert!(p0 <= 5);
}

#[test]
fn percentile_one_returns_maximum() {
    let mut h = HdrHistogram::new(3);
    for i in 1..=100u64 {
        h.record(i);
    }
    assert!(h.value_at_percentile(1.0) >= 95);
}

#[test]
fn count_is_zero_on_create() {
    assert_eq!(HdrHistogram::new(3).count(), 0);
    assert_eq!(HdrHistogram::new(1).count(), 0);
    assert_eq!(HdrHistogram::new(5).count(), 0);
}

#[test]
fn percentile_clamped_above_one() {
    let mut h = HdrHistogram::new(3);
    h.record(42);
    assert_eq!(h.value_at_percentile(1.5), h.value_at_percentile(1.0));
}

#[test]
fn high_volume_record_stays_consistent() {
    let mut h = HdrHistogram::new(3);
    for i in 0..100_000u64 {
        h.record((i % 1000) + 1);
    }
    assert_eq!(h.count(), 100_000);
    let p50 = h.value_at_percentile(0.5);
    assert!((400..=600).contains(&p50), "p50 in mid-range, got {p50}");
}

#[test]
fn co_no_correction_when_value_at_or_below_interval() {
    let mut h = HdrHistogram::new(3);
    h.record_with_expected_interval(5, 10); // value < interval: no backfill
    h.record_with_expected_interval(10, 10); // value == interval: no backfill
    assert_eq!(h.count(), 2, "on-cadence samples add exactly one each");
}

#[test]
fn co_disabled_when_interval_zero() {
    let mut h = HdrHistogram::new(3);
    h.record_with_expected_interval(1000, 0); // interval 0: plain record
    assert_eq!(h.count(), 1);
}

#[test]
fn co_backfills_the_stall() {
    let mut h = HdrHistogram::new(3);
    // A 1000-unit op at a 10-unit expected cadence backfills the requests the
    // generator could not issue: 990, 980, ..., 10 (99) plus the 1000 itself.
    h.record_with_expected_interval(1000, 10);
    assert_eq!(h.count(), 100, "1 real + 99 synthetic samples");
}

#[test]
fn co_correction_lifts_the_tail() {
    // 1000 on-cadence ops, then one long stall. Coordinated omission would hide
    // the stall's blast radius; the correction exposes it in the tail.
    let mut plain = HdrHistogram::new(3);
    let mut corrected = HdrHistogram::new(3);
    for _ in 0..1000 {
        plain.record(10);
        corrected.record_with_expected_interval(10, 10);
    }
    plain.record(1000);
    corrected.record_with_expected_interval(1000, 10);

    let p99_plain = plain.value_at_percentile(0.99);
    let p99_corrected = corrected.value_at_percentile(0.99);
    assert!(
        p99_plain <= 20,
        "uncorrected p99 hides the stall: {p99_plain}"
    );
    assert!(
        p99_corrected > 100,
        "corrected p99 reflects the requests the stall blocked: {p99_corrected}"
    );
}

#[test]
fn empty_stats_are_zero() {
    let h = HdrHistogram::new(3);
    assert_eq!(h.min(), 0);
    assert_eq!(h.mean(), 0.0);
    assert_eq!(h.count_at_value(42), 0);
    assert_eq!(h.percentile_at_or_below_value(42), 0.0);
}

#[test]
fn min_and_mean_track_the_distribution() {
    let mut h = HdrHistogram::new(3);
    for i in 1..=1000u64 {
        h.record(i);
    }
    // Values below sub_count are their own bucket, so 1..1000 is exact.
    assert_eq!(h.min(), 1);
    let mean = h.mean();
    assert!((500.0..=501.0).contains(&mean), "mean={mean}");
}

#[test]
fn count_at_value_reads_one_bucket() {
    let mut h = HdrHistogram::new(3);
    for _ in 0..7 {
        h.record(500);
    }
    h.record(9_000_000);
    assert_eq!(h.count_at_value(500), 7);
    assert_eq!(h.count_at_value(501), 0);
    // A value past the array end reads zero rather than panicking.
    assert_eq!(h.count_at_value(u64::MAX), 0);
    assert_eq!(h.count_at_value(9_000_000), 1);
}

#[test]
fn percentile_at_or_below_value_inverts_the_percentile_read() {
    let mut h = HdrHistogram::new(3);
    for i in 1..=1000u64 {
        h.record(i);
    }
    let q = h.percentile_at_or_below_value(500);
    assert!((0.49..=0.51).contains(&q), "q={q}");
    assert_eq!(h.percentile_at_or_below_value(1000), 1.0);
    assert!(h.percentile_at_or_below_value(u64::MAX) >= 1.0);
}

#[test]
fn footprint_grows_with_range_not_volume() {
    let mut small = HdrHistogram::new(3);
    for _ in 0..100_000u64 {
        small.record(999);
    }
    let base = small.footprint_bytes();
    assert_eq!(base, 2048 * 8, "array starts at sub_count counters");

    let mut wide = HdrHistogram::new(3);
    wide.record(1_000_000);
    assert!(
        wide.footprint_bytes() > base,
        "a wider range grows the array: {} vs {base}",
        wide.footprint_bytes()
    );
}

#[test]
fn reset_empties_without_shrinking() {
    let mut h = HdrHistogram::new(3);
    for i in 1..=1000u64 {
        h.record(i * 1000);
    }
    let footprint = h.footprint_bytes();
    h.reset();
    assert_eq!(h.count(), 0);
    assert_eq!(h.max(), 0);
    assert_eq!(h.min(), 0);
    assert_eq!(h.value_at_percentile(0.99), 0);
    assert_eq!(h.footprint_bytes(), footprint, "the array stays allocated");
    h.record(50);
    assert_eq!(h.count(), 1);
    assert_eq!(h.max(), 50);
}
