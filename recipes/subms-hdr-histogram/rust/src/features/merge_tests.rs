use super::*;

#[test]
fn empty_into_empty() {
    let mut a = HdrHistogram::new(3);
    let b = HdrHistogram::new(3);
    merge(&mut a, &b).unwrap();
    assert_eq!(a.count(), 0);
    assert_eq!(a.max(), 0);
}

#[test]
fn sums_counts() {
    let mut a = HdrHistogram::new(3);
    let mut b = HdrHistogram::new(3);
    for v in 1u64..=100 {
        a.record(v);
        b.record(v);
    }
    merge(&mut a, &b).unwrap();
    assert_eq!(a.count(), 200);
}

#[test]
fn equivalent_to_recording_all_values() {
    let mut a = HdrHistogram::new(3);
    let mut b = HdrHistogram::new(3);
    let mut single = HdrHistogram::new(3);
    for v in 1u64..=500 {
        a.record(v);
        single.record(v);
    }
    for v in 501u64..=1000 {
        b.record(v);
        single.record(v);
    }
    merge(&mut a, &b).unwrap();
    assert_eq!(a.count(), single.count());
    // p50 and p99 land in identical buckets.
    assert_eq!(a.value_at_percentile(0.5), single.value_at_percentile(0.5));
    assert_eq!(
        a.value_at_percentile(0.99),
        single.value_at_percentile(0.99)
    );
}

#[test]
fn mismatched_precision_errors() {
    let mut a = HdrHistogram::new(2);
    let b = HdrHistogram::new(4);
    let err = merge(&mut a, &b).unwrap_err();
    assert_eq!(err, "significant-digit mismatch");
}

#[test]
fn merge_grows_dst_to_fit_src() {
    let mut a = HdrHistogram::new(3);
    a.record(1);
    let mut b = HdrHistogram::new(3);
    let big = 100_000_000u64;
    b.record(big);
    merge(&mut a, &b).unwrap();
    assert_eq!(a.count(), 2);
    // Max should reflect big's bucket.
    assert!(a.max() >= big / 2, "merged max ~ big, got {}", a.max());
}

#[test]
fn merge_preserves_distribution_shape() {
    let mut a = HdrHistogram::new(3);
    let mut b = HdrHistogram::new(3);
    for _ in 0..1000 {
        a.record(50);
    }
    for _ in 0..1000 {
        b.record(500);
    }
    merge(&mut a, &b).unwrap();
    let p50 = a.value_at_percentile(0.5);
    let p99 = a.value_at_percentile(0.99);
    assert!(p50 < 100, "low half from a, got {p50}");
    assert!(p99 >= 400, "high tail from b, got {p99}");
}
