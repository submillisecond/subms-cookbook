//! Bootstrap confidence intervals. Behind the `bootstrap` Cargo
//! feature (on by default).
//!
//! Use this when reporting a percentile to know how WIDE the
//! confidence interval is - "p99 = 230us +- 50us" is meaningful;
//! "p99 = 230us" alone is missing the noise floor.

use crate::percentiles::percentile;

/// Bootstrap a percentile estimate with `iters` resamples, returning
/// the lower + upper bounds of the `confidence`-level interval.
/// `confidence` in `[0.0, 1.0]`; typical value `0.95`. Uses a
/// deterministic LCG (`SubMsLcg`-compatible constants) so results
/// are reproducible across runs given the same `seed`.
///
/// Cost is `O(iters * n)`; defaults that work well in practice:
/// `iters = 200..1000`, `confidence = 0.95`.
pub fn bootstrap_percentile_ci(
    samples: &[u64],
    q: f64,
    iters: usize,
    confidence: f64,
    seed: u64,
) -> (u64, u64) {
    if samples.is_empty() || iters == 0 {
        return (0, 0);
    }
    let mut state = seed | 1;
    let mut estimates = Vec::with_capacity(iters);
    for _ in 0..iters {
        let mut resample = Vec::with_capacity(samples.len());
        for _ in 0..samples.len() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = (state as usize) % samples.len();
            resample.push(samples[idx]);
        }
        resample.sort_unstable();
        estimates.push(percentile(&resample, q));
    }
    estimates.sort_unstable();
    let lo_q = (1.0 - confidence) / 2.0;
    let hi_q = 1.0 - lo_q;
    (percentile(&estimates, lo_q), percentile(&estimates, hi_q))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p99_ci_brackets_point_estimate() {
        let v: Vec<u64> = (0..1000).collect();
        let (lo, hi) = bootstrap_percentile_ci(&v, 0.99, 200, 0.95, 42);
        let point = {
            let mut s = v.clone();
            s.sort_unstable();
            percentile(&s, 0.99)
        };
        assert!(lo <= point && point <= hi);
    }

    #[test]
    fn empty_returns_zero_pair() {
        assert_eq!(bootstrap_percentile_ci(&[], 0.99, 100, 0.95, 0), (0, 0));
    }

    #[test]
    fn deterministic_under_same_seed() {
        let v: Vec<u64> = (0..200).collect();
        let a = bootstrap_percentile_ci(&v, 0.99, 100, 0.95, 7);
        let b = bootstrap_percentile_ci(&v, 0.99, 100, 0.95, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn zero_iters_returns_zero_pair() {
        let v: Vec<u64> = (0..100).collect();
        assert_eq!(bootstrap_percentile_ci(&v, 0.99, 0, 0.95, 0), (0, 0));
    }
}
