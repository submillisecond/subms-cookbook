//! Tail analysis: the bit institutional users actually care about.
//! Behind the `tail` Cargo feature (on by default).

use crate::percentiles::percentile;

/// Conditional tail expectation: the mean of all samples ABOVE the
/// given quantile. Also known as expected shortfall (ES) or
/// conditional value-at-risk (CVaR). For latency: "given that we're
/// in the worst N% of cases, what does that average look like?"
///
/// `q` in `[0.0, 1.0]`. Returns 0 if no samples fall above the
/// quantile threshold.
pub fn conditional_tail_expectation(samples: &[u64], q: f64) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let cutoff = percentile(&sorted, q);
    let tail: Vec<u64> = sorted.into_iter().filter(|&v| v > cutoff).collect();
    if tail.is_empty() {
        return cutoff;
    }
    tail.iter().sum::<u64>() / tail.len() as u64
}

/// Hill estimator for the tail index. Higher values mean a heavier
/// tail (more frequent extreme outliers). Uses the top `k` order
/// statistics. Returns `None` if `k < 2` or fewer than `k+1` samples.
///
/// Interpretation: a Hill index above 1.0 indicates a power-law tail
/// where occasional 10x-typical outliers should be expected; below
/// 0.3 indicates a near-exponential tail with rare outliers.
pub fn hill_tail_index(samples: &[u64], k: usize) -> Option<f64> {
    if k < 2 || samples.len() <= k {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let top: Vec<u64> = sorted.iter().rev().take(k + 1).copied().collect();
    let pivot = top[k];
    if pivot == 0 {
        return None;
    }
    let pivot_f = pivot as f64;
    let mut sum = 0.0;
    for v in top.iter().take(k) {
        let ratio = (*v as f64) / pivot_f;
        if ratio > 0.0 {
            sum += ratio.ln();
        }
    }
    Some(sum / k as f64)
}

/// Ratio of p99 over p50. Quick "how fat is the tail" indicator:
/// 1.0 means uniform, 10+ means a meaningful tail. Returns 0 when
/// p50 is 0.
pub fn tail_fatness_ratio(samples: &[u64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p50 = percentile(&sorted, 0.50);
    let p99 = percentile(&sorted, 0.99);
    if p50 == 0 {
        return 0.0;
    }
    p99 as f64 / p50 as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cte_exceeds_quantile() {
        let v: Vec<u64> = (0..100).collect();
        let cte99 = conditional_tail_expectation(&v, 0.95);
        assert!(cte99 >= 95);
    }

    #[test]
    fn cte_empty_is_zero() {
        assert_eq!(conditional_tail_expectation(&[], 0.99), 0);
    }

    #[test]
    fn hill_returns_none_for_tiny_input() {
        assert!(hill_tail_index(&[1, 2, 3], 5).is_none());
    }

    #[test]
    fn hill_powerlike_tail_returns_positive() {
        let v: Vec<u64> = (1..1000).map(|i| (i as u64).pow(2)).collect();
        let idx = hill_tail_index(&v, 50).unwrap();
        assert!(
            idx > 0.0,
            "Hill estimator on power-law tail should be positive: {}",
            idx
        );
    }

    #[test]
    fn fatness_ratio_uniform_close_to_one() {
        let v = vec![100u64; 1000];
        let r = tail_fatness_ratio(&v);
        assert!((r - 1.0).abs() < 0.01);
    }

    #[test]
    fn fatness_ratio_heavy_tail_exceeds_one() {
        let mut v: Vec<u64> = vec![100; 990];
        v.resize(v.len() + 10, 10_000);
        let r = tail_fatness_ratio(&v);
        assert!(r > 1.0);
    }
}
