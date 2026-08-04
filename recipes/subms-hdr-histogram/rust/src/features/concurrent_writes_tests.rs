use super::*;
use std::sync::Arc;
use std::thread;

#[test]
fn single_thread_records() {
    let h = ConcurrentHdrHistogram::new(3);
    for v in [10u64, 20, 30, 40, 50] {
        h.record(v);
    }
    assert_eq!(h.count(), 5);
    assert!(h.max() >= 50);
}

#[test]
fn empty_returns_zero() {
    let h = ConcurrentHdrHistogram::new(3);
    assert_eq!(h.count(), 0);
    assert_eq!(h.max(), 0);
    assert_eq!(h.value_at_percentile(0.99), 0);
}

#[test]
fn shape_accessors_match_precision() {
    let h = ConcurrentHdrHistogram::new(3);
    assert_eq!(h.sub_count(), 1u32 << h.sub_count_bits());
    assert!(h.sub_count() >= 2);
}

#[test]
fn percentiles_match_distribution() {
    let h = ConcurrentHdrHistogram::new(3);
    for i in 1..=1000 {
        h.record(i);
    }
    let p50 = h.value_at_percentile(0.5);
    let p99 = h.value_at_percentile(0.99);
    assert!((450..=550).contains(&p50), "p50={p50}");
    assert!((950..=1050).contains(&p99), "p99={p99}");
}

#[test]
fn concurrent_writers_lose_nothing() {
    let h = Arc::new(ConcurrentHdrHistogram::new(3));
    let threads = 8;
    let per_thread = 25_000;
    let mut handles = vec![];
    for t in 0..threads {
        let h = h.clone();
        handles.push(thread::spawn(move || {
            for i in 0..per_thread {
                h.record(((t * per_thread + i) as u64 % 1000) + 1);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(h.count(), (threads * per_thread) as u64);
    let p99 = h.value_at_percentile(0.99);
    assert!(p99 >= 900, "p99 in expected range, got {p99}");
}

#[test]
fn snapshot_preserves_total() {
    let h = ConcurrentHdrHistogram::new(3);
    for i in 1..=100 {
        h.record(i);
    }
    let snap = h.drain_snapshot();
    assert_eq!(snap.count(), 100);
    assert_eq!(h.count(), 0, "live side cleared after drain");
    let p99 = snap.value_at_percentile(0.99);
    assert!(p99 >= 95, "snapshot p99 ~ 99, got {p99}");
}

#[test]
fn snapshot_then_record_starts_fresh() {
    let h = ConcurrentHdrHistogram::new(3);
    for i in 1..=10 {
        h.record(i);
    }
    let _ = h.drain_snapshot();
    h.record(500);
    assert_eq!(h.count(), 1);
    assert!(h.max() >= 500);
}

#[test]
fn clamps_above_bucket_capacity() {
    // Tiny: 1 sig-digit + 1 major.
    let h = ConcurrentHdrHistogram::with_majors(1, 1);
    let huge = u64::MAX / 2;
    h.record(huge);
    assert_eq!(h.count(), 1);
    // Should not panic; max returns the last bucket's value.
    let _ = h.max();
}

#[test]
fn empty_snapshot_reads_zero() {
    let s = ConcurrentHdrHistogram::new(3).drain_snapshot();
    assert_eq!(s.count(), 0);
    assert_eq!(s.max(), 0);
    assert_eq!(s.value_at_percentile(0.99), 0);
}

#[test]
fn snapshot_percentiles_match_the_drained_distribution() {
    let h = ConcurrentHdrHistogram::new(3);
    for i in 1..=1000u64 {
        h.record(i);
    }
    let s = h.drain_snapshot();
    assert_eq!(s.count(), 1000);
    assert_eq!(s.max(), 1000);
    let p50 = s.value_at_percentile(0.50);
    let p99 = s.value_at_percentile(0.99);
    assert!((450..=550).contains(&p50), "p50={p50}");
    assert!((950..=1050).contains(&p99), "p99={p99}");
}

#[test]
fn snapshot_quantile_is_clamped_at_both_ends() {
    let h = ConcurrentHdrHistogram::new(3);
    for i in 1..=100u64 {
        h.record(i);
    }
    let s = h.drain_snapshot();
    assert_eq!(s.value_at_percentile(-1.0), 1, "below 0 reads the minimum");
    assert_eq!(s.value_at_percentile(2.0), 100, "above 1 reads the maximum");
}

#[test]
fn snapshot_large_values_round_trip_through_the_bucket_inverse() {
    let h = ConcurrentHdrHistogram::new(3);
    h.record(9_000_000);
    let s = h.drain_snapshot();
    let max = s.max();
    assert!(
        (8_950_000..=9_000_000).contains(&max),
        "the bucket lower bound sits inside the error band: {max}"
    );
}
