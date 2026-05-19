use subms_hdr_histogram::HdrHistogram;

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
    assert!(p50 >= 450 && p50 <= 550, "p50={p50}");
    assert!(p99 >= 950 && p99 <= 1050, "p99={p99}");
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
    assert!(p50 >= 400 && p50 <= 600, "p50 in mid-range, got {p50}");
}
