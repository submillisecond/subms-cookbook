use super::*;

#[test]
fn empty_returns_zero() {
    let h = TaggedHdrHistogram::new(3);
    assert_eq!(h.count(), 0);
    assert_eq!(h.count_for_tag(1), 0);
    assert_eq!(h.value_at_percentile(0.99), 0);
    assert_eq!(h.value_at_percentile_for_tag(0.99, 1), 0);
    assert_eq!(h.max(), 0);
}

#[test]
fn records_tagged_counts() {
    let mut h = TaggedHdrHistogram::new(3);
    for v in 1u64..=10 {
        h.record(v, 1);
    }
    for v in 100u64..=109 {
        h.record(v, 2);
    }
    assert_eq!(h.count(), 20);
    assert_eq!(h.count_for_tag(1), 10);
    assert_eq!(h.count_for_tag(2), 10);
    assert_eq!(h.count_for_tag(3), 0);
}

#[test]
fn per_tag_percentiles_are_separate() {
    let mut h = TaggedHdrHistogram::new(3);
    // Tag 1: small values.
    for v in 1u64..=1000 {
        h.record(v, 1);
    }
    // Tag 2: big values.
    for v in 10_000u64..=11_000 {
        h.record(v, 2);
    }
    let p99_a = h.value_at_percentile_for_tag(0.99, 1);
    let p99_b = h.value_at_percentile_for_tag(0.99, 2);
    assert!(p99_a < 1100, "tag 1 p99 small: {p99_a}");
    assert!(p99_b >= 10_000, "tag 2 p99 large: {p99_b}");
}

#[test]
fn aggregate_percentile_spans_all_tags() {
    let mut h = TaggedHdrHistogram::new(3);
    for v in 1u64..=500 {
        h.record(v, 1);
    }
    for v in 501u64..=1000 {
        h.record(v, 2);
    }
    assert_eq!(h.count(), 1000);
    let p50 = h.value_at_percentile(0.5);
    assert!((450..=550).contains(&p50), "aggregate p50={p50}");
}

#[test]
fn tags_listing_returns_unique() {
    let mut h = TaggedHdrHistogram::new(3);
    h.record(10, 1);
    h.record(20, 2);
    h.record(30, 1);
    h.record(40, 3);
    let mut tags = h.tags();
    tags.sort();
    assert_eq!(tags, vec![1u8, 2, 3]);
}

#[test]
fn unknown_tag_percentile_is_zero() {
    let mut h = TaggedHdrHistogram::new(3);
    h.record(50, 1);
    assert_eq!(h.value_at_percentile_for_tag(0.5, 99), 0);
}

#[test]
fn many_tags_per_bucket() {
    let mut h = TaggedHdrHistogram::new(3);
    // All records land at value=50 - same bucket - across 10 tags.
    for tag in 0u8..10 {
        for _ in 0..100 {
            h.record(50, tag);
        }
    }
    assert_eq!(h.count(), 1000);
    for tag in 0u8..10 {
        assert_eq!(h.count_for_tag(tag), 100);
        assert!(h.value_at_percentile_for_tag(0.99, tag) > 0);
    }
}
