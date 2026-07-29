//! Jitter score: was the measurement *environment* stable across the
//! run? Behind the `jitter` Cargo feature (on by default).

/// Jitter score: coefficient of variation of the per-window mean
/// across non-overlapping 32-sample windows. Clamped to `[0.0, 1.0]`.
/// Returns 0 when there are fewer than 64 samples (two windows).
///
/// The score answers: "did the *measurement* noise vary across the
/// run?" A high jitter score doesn't mean the algorithm is slow - it
/// means the numbers shifted under our feet (GC, NUMA migration, CPU
/// thermal throttling, OS scheduler preemption). Separate signal from
/// the per-stage p99 tail.
pub fn jitter_score(samples: &[u64]) -> f64 {
    const WIN: usize = 32;
    if samples.len() < WIN * 2 {
        return 0.0;
    }
    let windows = samples.len() / WIN;
    let mut means = Vec::with_capacity(windows);
    for w in 0..windows {
        let start = w * WIN;
        let slice = &samples[start..start + WIN];
        let sum: u64 = slice.iter().sum();
        means.push(sum as f64 / WIN as f64);
    }
    let grand_mean: f64 = means.iter().sum::<f64>() / means.len() as f64;
    if grand_mean <= 0.0 {
        return 0.0;
    }
    let variance: f64 =
        means.iter().map(|m| (m - grand_mean).powi(2)).sum::<f64>() / means.len() as f64;
    let cv = variance.sqrt() / grand_mean;
    cv.clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "jitter_tests.rs"]
mod tests;
