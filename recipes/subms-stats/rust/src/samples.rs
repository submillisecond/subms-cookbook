//! Ergonomic wrapper around a borrowed `&[u64]` slice of nanosecond
//! latency samples. Method-chained API for the common statistics.
//!
//! The set of methods available on `SubMsSamples` depends on which
//! Cargo features are enabled. Always-on: `count`, `is_empty`, `raw`,
//! `p50` / `p99` / `p999` / `percentile` / `percentile_sweep`,
//! `mean`, `stddev`, `max`. Feature-gated methods are documented at
//! their definitions.
//!
//! ```
//! use subms_stats::SubMsSamples;
//!
//! let raw = vec![100u64, 200, 150, 300, 250, 175, 125, 400];
//! let s = SubMsSamples::new(&raw);
//! let p50 = s.p50();
//! let p99 = s.p99();
//! assert!(p99 >= p50);
//! ```

use crate::percentiles;

/// Borrowing facade over a sample slice. Zero-cost - holds a
/// reference only. Lazy-sorts internally on demand for percentile
/// queries.
#[derive(Clone, Copy)]
pub struct SubMsSamples<'a> {
    raw: &'a [u64],
}

impl<'a> SubMsSamples<'a> {
    /// Wrap a borrowed slice of nanosecond samples.
    pub fn new(raw: &'a [u64]) -> Self {
        Self { raw }
    }

    /// Number of samples in the underlying slice.
    pub fn count(&self) -> usize {
        self.raw.len()
    }

    /// `true` if the underlying slice is empty.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Raw chronological sample buffer.
    pub fn raw(&self) -> &'a [u64] {
        self.raw
    }

    /// 50th percentile (median).
    pub fn p50(&self) -> u64 {
        self.percentile(0.50)
    }

    /// 90th percentile.
    pub fn p90(&self) -> u64 {
        self.percentile(0.90)
    }

    /// 99th percentile - the standard tail-latency headline.
    pub fn p99(&self) -> u64 {
        self.percentile(0.99)
    }

    /// 99.9th percentile - the "worst typical case".
    pub fn p999(&self) -> u64 {
        self.percentile(0.999)
    }

    /// Maximum observed sample, `0` if empty.
    pub fn max(&self) -> u64 {
        self.raw.iter().copied().max().unwrap_or(0)
    }

    /// Arithmetic mean. `0` if empty.
    pub fn mean(&self) -> u64 {
        percentiles::mean(self.raw)
    }

    /// Sample standard deviation (n-1).
    pub fn stddev(&self) -> u64 {
        percentiles::stddev(self.raw)
    }

    /// Arbitrary quantile in `[0.0, 1.0]`. Sorts internally.
    pub fn percentile(&self, q: f64) -> u64 {
        let mut sorted = self.raw.to_vec();
        sorted.sort_unstable();
        percentiles::percentile(&sorted, q)
    }

    /// Sweep across a quantile range.
    pub fn percentile_sweep(&self, start: f64, end: f64, step: f64) -> Vec<(f64, u64)> {
        percentiles::percentile_sweep(self.raw, start, end, step)
    }

    /// Log2-spaced CDF buckets. Only available with the `histogram`
    /// feature.
    #[cfg(feature = "histogram")]
    pub fn cdf_buckets(&self) -> Vec<u64> {
        crate::histogram::cdf_buckets(self.raw)
    }

    /// Measurement-rig jitter score in `[0.0, 1.0]`. Only available
    /// with the `jitter` feature.
    #[cfg(feature = "jitter")]
    pub fn jitter_score(&self) -> f64 {
        crate::jitter::jitter_score(self.raw)
    }

    /// Conditional tail expectation. Only with `tail` feature.
    #[cfg(feature = "tail")]
    pub fn conditional_tail_expectation(&self, q: f64) -> u64 {
        crate::tail::conditional_tail_expectation(self.raw, q)
    }

    /// Tail fatness ratio (p99 / p50). Only with `tail` feature.
    #[cfg(feature = "tail")]
    pub fn tail_fatness_ratio(&self) -> f64 {
        crate::tail::tail_fatness_ratio(self.raw)
    }

    /// Hill estimator. Only with `tail` feature.
    #[cfg(feature = "tail")]
    pub fn hill_tail_index(&self, k: usize) -> Option<f64> {
        crate::tail::hill_tail_index(self.raw, k)
    }

    /// Interquartile range. Only with `robust` feature.
    #[cfg(feature = "robust")]
    pub fn iqr(&self) -> u64 {
        crate::robust::iqr(self.raw)
    }

    /// Median absolute deviation. Only with `robust` feature.
    #[cfg(feature = "robust")]
    pub fn median_absolute_deviation(&self) -> u64 {
        crate::robust::median_absolute_deviation(self.raw)
    }

    /// Coefficient of variation. Only with `robust` feature.
    #[cfg(feature = "robust")]
    pub fn coefficient_of_variation(&self) -> f64 {
        crate::robust::coefficient_of_variation(self.raw)
    }

    /// Skewness (3rd standardised moment). Only with `robust` feature.
    #[cfg(feature = "robust")]
    pub fn skewness(&self) -> f64 {
        crate::robust::skewness(self.raw)
    }

    /// Excess kurtosis. Only with `robust` feature.
    #[cfg(feature = "robust")]
    pub fn kurtosis(&self) -> f64 {
        crate::robust::kurtosis(self.raw)
    }

    /// Bootstrap CI for a percentile. Only with `bootstrap` feature.
    #[cfg(feature = "bootstrap")]
    pub fn bootstrap_percentile_ci(
        &self,
        q: f64,
        iters: usize,
        confidence: f64,
        seed: u64,
    ) -> (u64, u64) {
        crate::bootstrap::bootstrap_percentile_ci(self.raw, q, iters, confidence, seed)
    }
}

impl<'a> From<&'a [u64]> for SubMsSamples<'a> {
    fn from(raw: &'a [u64]) -> Self {
        Self::new(raw)
    }
}

impl<'a> From<&'a Vec<u64>> for SubMsSamples<'a> {
    fn from(raw: &'a Vec<u64>) -> Self {
        Self::new(raw.as_slice())
    }
}

#[cfg(test)]
mod tests {
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
}
