//! Core percentiles + first-moment statistics. Always available; no
//! Cargo feature gate.
//!
//! All functions take `&[u64]` and are pure. `percentile` expects a
//! sorted slice; `percentile_sweep` and `mean` / `stddev` accept
//! unsorted input.

/// Percentile over a *sorted* ns array. Empty -> 0. Index is
/// `min(n-1, floor(q*n))` so `q = 1.0` returns the max.
///
/// Caller MUST sort the slice first. (Choosing not to sort internally
/// lets the caller batch many percentile queries against one
/// pre-sorted buffer.)
pub fn percentile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((q * sorted.len() as f64) as usize).min(sorted.len() - 1);
    sorted[idx]
}

/// Compute percentiles across a uniform quantile range, returning
/// `(quantile, ns)` pairs. `start`/`end` in `[0.0, 1.0]`. Includes
/// both endpoints.
///
/// Example: `percentile_sweep(&samples, 0.50, 0.999, 0.05)` returns
/// p50, p55, p60, ..., p95, p99.9. Use for full-CDF plots or for
/// asserting a custom quantile threshold that p50/p99/p999 doesn't
/// cover.
pub fn percentile_sweep(samples: &[u64], start: f64, end: f64, step: f64) -> Vec<(f64, u64)> {
    if samples.is_empty() || step <= 0.0 {
        return Vec::new();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let mut out = Vec::new();
    let mut q = start;
    while q <= end + 1e-9 {
        out.push((q, percentile(&sorted, q)));
        q += step;
    }
    out
}

/// Arithmetic mean. Empty -> 0. Integer rounding via division; the
/// summary writer rounds to whole ns deliberately because the JSON
/// contract emits integers.
pub fn mean(samples: &[u64]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.iter().sum::<u64>() / samples.len() as u64
}

/// Sample standard deviation (n-1 denominator). 0 when `count < 2`.
/// Uses f64 internally so the sum-of-squares doesn't overflow on a
/// stage that records millions of small ns values.
pub fn stddev(samples: &[u64]) -> u64 {
    let n = samples.len();
    if n < 2 {
        return 0;
    }
    let mean_f = mean(samples) as f64;
    let mut variance = 0.0f64;
    for &v in samples {
        let d = v as f64 - mean_f;
        variance += d * d;
    }
    variance /= (n - 1) as f64;
    variance.sqrt().round() as u64
}

#[cfg(test)]
#[path = "percentiles_tests.rs"]
mod tests;
