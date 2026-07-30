use super::*;

#[test]
fn linear_visits_only_populated_buckets() {
    let mut h = HdrHistogram::new(3);
    for v in [10u64, 100, 1000] {
        h.record(v);
    }
    let entries: Vec<IterEntry> = h.iter_linear().collect();
    assert_eq!(entries.len(), 3, "exactly the three populated buckets");
    // value_lo strictly increasing.
    for w in entries.windows(2) {
        assert!(w[0].value_lo < w[1].value_lo, "value order");
    }
    // Cumulative ends at total.
    assert_eq!(entries.last().unwrap().cumulative, 3);
}

#[test]
fn linear_on_empty_yields_nothing() {
    let h = HdrHistogram::new(3);
    let entries: Vec<IterEntry> = h.iter_linear().collect();
    assert!(entries.is_empty());
}

#[test]
fn logarithmic_bands_double() {
    let mut h = HdrHistogram::new(3);
    for v in [1u64, 3, 7, 15, 31, 100, 1000] {
        h.record(v);
    }
    let entries: Vec<IterEntry> = h.iter_logarithmic().collect();
    assert!(!entries.is_empty());
    // Each band's hi must equal the next band's lo (or 2*lo).
    for w in entries.windows(2) {
        assert_eq!(w[1].value_lo, w[0].value_hi, "abutting bands");
        assert_eq!(w[0].value_hi, w[0].value_lo * 2, "powers of two");
    }
    // Total counts across bands = total recorded.
    let total: u64 = entries.iter().map(|e| e.count).sum();
    assert_eq!(total, 7, "every record covered by some band");
}

#[test]
fn logarithmic_covers_high_bucket() {
    let mut h = HdrHistogram::new(3);
    h.record(1);
    h.record(1_000_000);
    let entries: Vec<IterEntry> = h.iter_logarithmic().collect();
    // The last band must cover the 1M record.
    let total: u64 = entries.iter().map(|e| e.count).sum();
    assert_eq!(total, 2);
}

#[test]
fn percentile_emits_roughly_step_entries() {
    let mut h = HdrHistogram::new(3);
    for v in 1u64..=1000 {
        h.record(v);
    }
    let entries: Vec<IterEntry> = h.iter_percentiles(10.0).collect();
    // 10% step on 100% coverage gives ~10 entries.
    assert!(
        (8..=12).contains(&entries.len()),
        "got {} percentile entries",
        entries.len()
    );
    // Percentile cumulative is monotonic.
    for w in entries.windows(2) {
        assert!(w[0].cumulative <= w[1].cumulative);
    }
}

#[test]
fn percentile_on_empty_yields_nothing() {
    let h = HdrHistogram::new(3);
    let entries: Vec<IterEntry> = h.iter_percentiles(1.0).collect();
    assert!(entries.is_empty());
}

#[test]
fn linear_and_percentile_share_population() {
    let mut h = HdrHistogram::new(3);
    for v in 1u64..=100 {
        h.record(v);
    }
    let linear_total: u64 = h.iter_linear().map(|e| e.count).sum();
    assert_eq!(linear_total, 100);
    let last_pct = h.iter_percentiles(1.0).last();
    assert!(last_pct.is_some());
}
