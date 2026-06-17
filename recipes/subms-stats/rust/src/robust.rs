//! Robust statistics: low sensitivity to outliers, useful when the
//! raw distribution has a heavy tail (which latency typically does).
//! Behind the `robust` Cargo feature (on by default).

use crate::percentiles::{mean, percentile, stddev};

/// Interquartile range (p75 - p25). The "middle 50%" spread. More
/// robust to outliers than stddev.
pub fn iqr(samples: &[u64]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p25 = percentile(&sorted, 0.25);
    let p75 = percentile(&sorted, 0.75);
    p75.saturating_sub(p25)
}

/// Median absolute deviation: the median of `|x - median(x)|`. Robust
/// alternative to stddev. Returns 0 for empty input.
pub fn median_absolute_deviation(samples: &[u64]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let median = percentile(&sorted, 0.50);
    let mut devs: Vec<u64> = sorted.iter().map(|&v| v.abs_diff(median)).collect();
    devs.sort_unstable();
    percentile(&devs, 0.50)
}

/// Coefficient of variation: stddev / mean. Unit-free measure of
/// relative variability. `0.0` for empty / zero-mean input.
pub fn coefficient_of_variation(samples: &[u64]) -> f64 {
    let m = mean(samples) as f64;
    if m <= 0.0 {
        return 0.0;
    }
    stddev(samples) as f64 / m
}

/// Skewness (3rd standardised moment). Positive skew means a right
/// tail (typical for latency distributions). Returns 0 with fewer
/// than 3 samples.
pub fn skewness(samples: &[u64]) -> f64 {
    let n = samples.len();
    if n < 3 {
        return 0.0;
    }
    let m = mean(samples) as f64;
    let mut s2 = 0.0f64;
    let mut s3 = 0.0f64;
    for &v in samples {
        let d = v as f64 - m;
        s2 += d * d;
        s3 += d * d * d;
    }
    let variance = s2 / n as f64;
    let std = variance.sqrt();
    if std <= 0.0 {
        return 0.0;
    }
    (s3 / n as f64) / std.powi(3)
}

/// Excess kurtosis (4th standardised moment minus 3). Positive
/// excess kurtosis means heavier tails than a normal distribution
/// (typical for latency). Returns 0 with fewer than 4 samples.
pub fn kurtosis(samples: &[u64]) -> f64 {
    let n = samples.len();
    if n < 4 {
        return 0.0;
    }
    let m = mean(samples) as f64;
    let mut s2 = 0.0f64;
    let mut s4 = 0.0f64;
    for &v in samples {
        let d = v as f64 - m;
        s2 += d * d;
        s4 += d * d * d * d;
    }
    let variance = s2 / n as f64;
    if variance <= 0.0 {
        return 0.0;
    }
    (s4 / n as f64) / variance.powi(2) - 3.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iqr_known_distribution() {
        let v: Vec<u64> = (0..100).collect();
        assert_eq!(iqr(&v), 50);
    }

    #[test]
    fn mad_basic() {
        let v: Vec<u64> = (0..100).collect();
        let mad = median_absolute_deviation(&v);
        assert!((20..=30).contains(&mad), "MAD around 25: {}", mad);
    }

    #[test]
    fn cov_constant_signal_is_zero() {
        let v = vec![100u64; 100];
        assert!(coefficient_of_variation(&v) < 0.001);
    }

    #[test]
    fn skewness_right_tail_positive() {
        let mut v: Vec<u64> = vec![100; 990];
        v.resize(v.len() + 10, 10_000);
        assert!(skewness(&v) > 0.0);
    }

    #[test]
    fn kurtosis_heavy_tail_positive() {
        let mut v: Vec<u64> = vec![100; 990];
        v.resize(v.len() + 10, 10_000);
        assert!(kurtosis(&v) > 0.0);
    }

    #[test]
    fn iqr_empty_zero() {
        assert_eq!(iqr(&[]), 0);
    }
}
