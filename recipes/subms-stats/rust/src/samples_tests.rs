use super::*;

#[test]
fn empty_samples_returns_zero() {
    let raw: Vec<u64> = Vec::new();
    let s = SubMsSamples::new(&raw);
    assert_eq!(s.count(), 0);
    assert!(s.is_empty());
    assert_eq!(s.p99(), 0);
    assert_eq!(s.mean(), 0);
    assert_eq!(s.stddev(), 0);
    assert_eq!(s.max(), 0);
}

#[test]
fn known_distribution_percentiles() {
    let raw: Vec<u64> = (0..100).collect();
    let s = SubMsSamples::new(&raw);
    assert_eq!(s.count(), 100);
    assert_eq!(s.p50(), 50);
    assert_eq!(s.p99(), 99);
    assert_eq!(s.max(), 99);
}

#[test]
fn from_slice_and_vec_both_work() {
    let v: Vec<u64> = vec![100, 200, 300];
    let by_slice: SubMsSamples<'_> = (&v[..]).into();
    let by_vec: SubMsSamples<'_> = (&v).into();
    assert_eq!(by_slice.p50(), by_vec.p50());
}

#[test]
fn raw_returns_the_underlying_slice_unsorted() {
    let v: Vec<u64> = vec![300, 100, 200];
    let s = SubMsSamples::new(&v);
    assert_eq!(s.raw(), &[300, 100, 200]);
}

#[test]
fn facade_percentile_helpers_are_monotone() {
    let raw: Vec<u64> = (0..1000).collect();
    let s = SubMsSamples::new(&raw);
    assert!(s.p90() >= s.p50());
    assert!(s.p999() >= s.p99());
    assert!(s.p99() >= s.p90());
    let sweep = s.percentile_sweep(0.90, 0.99, 0.03);
    assert!(!sweep.is_empty());
    for w in sweep.windows(2) {
        assert!(w[1].1 >= w[0].1);
    }
}

#[cfg(feature = "histogram")]
#[test]
fn facade_cdf_buckets_count_every_sample() {
    let raw: Vec<u64> = (1..=500).collect();
    let buckets = SubMsSamples::new(&raw).cdf_buckets();
    assert_eq!(buckets.len(), 64);
    assert_eq!(buckets.iter().sum::<u64>(), raw.len() as u64);
}

#[cfg(feature = "jitter")]
#[test]
fn facade_jitter_score_in_unit_interval() {
    let raw: Vec<u64> = (0..256).map(|i| 100 + (i % 7)).collect();
    let score = SubMsSamples::new(&raw).jitter_score();
    assert!((0.0..=1.0).contains(&score));
}

#[cfg(feature = "tail")]
#[test]
fn facade_tail_methods_agree_with_module() {
    let raw: Vec<u64> = (1..=1000).collect();
    let s = SubMsSamples::new(&raw);
    assert_eq!(
        s.conditional_tail_expectation(0.99),
        crate::tail::conditional_tail_expectation(&raw, 0.99)
    );
    assert!(s.tail_fatness_ratio() >= 1.0);
    let hill = s.hill_tail_index(100);
    assert_eq!(hill, crate::tail::hill_tail_index(&raw, 100));
}

#[cfg(feature = "robust")]
#[test]
fn facade_robust_methods_agree_with_module() {
    let raw: Vec<u64> = (1..=1000).collect();
    let s = SubMsSamples::new(&raw);
    assert_eq!(s.iqr(), crate::robust::iqr(&raw));
    assert_eq!(
        s.median_absolute_deviation(),
        crate::robust::median_absolute_deviation(&raw)
    );
    assert_eq!(
        s.coefficient_of_variation(),
        crate::robust::coefficient_of_variation(&raw)
    );
    assert_eq!(s.skewness(), crate::robust::skewness(&raw));
    assert_eq!(s.kurtosis(), crate::robust::kurtosis(&raw));
}

#[cfg(feature = "bootstrap")]
#[test]
fn facade_bootstrap_ci_brackets_the_point_estimate() {
    let raw: Vec<u64> = (1..=1000).collect();
    let s = SubMsSamples::new(&raw);
    let (lo, hi) = s.bootstrap_percentile_ci(0.99, 200, 0.95, 42);
    let point = s.p99();
    assert!(lo <= point && point <= hi);
}
