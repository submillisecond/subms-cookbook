//! Distribution comparison: "is candidate slower than baseline?"
//! Behind the `compare` Cargo feature (on by default).

use crate::percentiles::{mean, stddev};

/// Two-sample Kolmogorov-Smirnov D statistic: the maximum vertical
/// gap between the two empirical CDFs. Range `[0.0, 1.0]`; larger
/// means more different. `None` if either side is empty.
///
/// Use to detect "did the distribution shift between baseline and
/// candidate?" - more sensitive than comparing p99 alone, since a
/// distribution can shift its body without moving p99 (or vice
/// versa).
pub fn ks_statistic(baseline: &[u64], candidate: &[u64]) -> Option<f64> {
    if baseline.is_empty() || candidate.is_empty() {
        return None;
    }
    let mut a = baseline.to_vec();
    let mut b = candidate.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    let mut i = 0usize;
    let mut j = 0usize;
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let mut max_d = 0.0f64;
    while i < a.len() && j < b.len() {
        if a[i] <= b[j] {
            i += 1;
        } else {
            j += 1;
        }
        let cdf_a = i as f64 / na;
        let cdf_b = j as f64 / nb;
        let d = (cdf_a - cdf_b).abs();
        if d > max_d {
            max_d = d;
        }
    }
    Some(max_d)
}

/// Cohen's d effect size: standardised difference between two means.
/// Magnitude `< 0.2` is small, `~ 0.5` medium, `> 0.8` large. Use
/// alongside [`ks_statistic`] to decide whether a p99 regression is
/// "real" or noise.
pub fn cohens_d(baseline: &[u64], candidate: &[u64]) -> Option<f64> {
    if baseline.is_empty() || candidate.is_empty() {
        return None;
    }
    let na = baseline.len();
    let nb = candidate.len();
    let ma = mean(baseline) as f64;
    let mb = mean(candidate) as f64;
    let sa = stddev(baseline) as f64;
    let sb = stddev(candidate) as f64;
    let pooled =
        (((na - 1) as f64 * sa * sa + (nb - 1) as f64 * sb * sb) / ((na + nb - 2) as f64)).sqrt();
    if pooled <= 0.0 {
        return Some(0.0);
    }
    Some((mb - ma) / pooled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ks_same_distribution_near_zero() {
        let a: Vec<u64> = (0..1000).collect();
        let b: Vec<u64> = (0..1000).collect();
        assert!(ks_statistic(&a, &b).unwrap() < 0.01);
    }

    #[test]
    fn ks_shifted_distribution_large() {
        let a: Vec<u64> = (0..1000).collect();
        let b: Vec<u64> = (500..1500).collect();
        assert!(ks_statistic(&a, &b).unwrap() > 0.4);
    }

    #[test]
    fn ks_empty_returns_none() {
        assert!(ks_statistic(&[], &[1, 2]).is_none());
        assert!(ks_statistic(&[1, 2], &[]).is_none());
    }

    #[test]
    fn cohens_d_zero_for_identical() {
        let a: Vec<u64> = (0..100).collect();
        let b: Vec<u64> = (0..100).collect();
        assert!(cohens_d(&a, &b).unwrap().abs() < 0.01);
    }

    #[test]
    fn cohens_d_positive_when_candidate_slower() {
        let a: Vec<u64> = (100..200).collect();
        let b: Vec<u64> = (200..300).collect();
        assert!(cohens_d(&a, &b).unwrap() > 0.5);
    }
}
