//! Runnable example for the `subms-stats` recipe. Generates a synthetic
//! latency-sample stream with a configurable tail shape, then drives it
//! through the full `subms-stats` public surface so a reader can see every
//! analysis next to the workload that motivated it.
//!
//! The workload is small on purpose. The point of the example is the *shape*
//! of the analyses, not a state-of-the-art microbench. `Workload` is
//! deterministic given its seed: the same seed produces byte-identical
//! samples on any host, which is what lets the tests assert specific
//! comparison outcomes.
//!
//! Two `TailShape` variants cover the two regimes a real latency
//! distribution typically lands in:
//!
//! - `Uniformish` - tight body around `base_ns`, rare ~3x outliers. The
//!   "hot path on a quiet box" shape.
//! - `PowerLaw` - same body, but the outlier multiplier is drawn from a
//!   power-law so the worst 1% can run 10-50x typical. The "GC pause" or
//!   "scheduler-pre-empted" shape.
//!
//! [`analyse`] runs the entire `subms-stats` surface against one sample
//! stream and returns a typed [`StatsReport`]. [`compare`] runs the
//! two-sample analyses ([`subms_stats::ks_statistic`],
//! [`subms_stats::cohens_d`]) between a baseline and a candidate.

use subms_stats::{
    SubMsSamples, bootstrap_percentile_ci, cdf_buckets, coefficient_of_variation, cohens_d,
    conditional_tail_expectation, hill_tail_index, iqr, jitter_score, ks_statistic, kurtosis,
    median_absolute_deviation, percentile_sweep, skewness, tail_fatness_ratio,
};

/// Tail shape of the synthetic workload. Selects how outliers are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailShape {
    /// Tight body, rare bounded outliers (max ~3x base).
    Uniformish,
    /// Same body, power-law outlier multiplier. Models GC / scheduler tail.
    PowerLaw,
}

/// Synthetic latency-sample generator. Deterministic given `seed`.
///
/// `base_ns` is the nominal hot-path latency; ~95% of samples land within
/// `+- base_ns / 5` of it. The remaining ~5% are outliers drawn per
/// [`TailShape`]. The chronological order of returned samples is preserved
/// so [`jitter_score`] sees a realistic window structure.
#[derive(Clone, Copy, Debug)]
pub struct Workload {
    pub n: usize,
    pub base_ns: u64,
    pub shape: TailShape,
    pub seed: u64,
}

impl Workload {
    /// Materialise the sample stream as a `Vec<u64>`.
    pub fn generate(&self) -> Vec<u64> {
        let mut rng = Lcg::new(self.seed);
        let mut out = Vec::with_capacity(self.n);
        // Spread of the body around base_ns. 20% lets stddev / IQR have
        // something to measure without swamping the tail signal.
        let body_spread = (self.base_ns / 5).max(1);
        for _ in 0..self.n {
            let coin = rng.next_f64();
            let v = if coin < 0.95 {
                let jitter = rng.bounded(2 * body_spread as u32 + 1) as i64 - body_spread as i64;
                (self.base_ns as i64 + jitter).max(1) as u64
            } else {
                match self.shape {
                    TailShape::Uniformish => {
                        // Outlier in [2x, 3x] base.
                        let mult = 2.0 + rng.next_f64();
                        ((self.base_ns as f64) * mult) as u64
                    }
                    TailShape::PowerLaw => {
                        // Inverse-CDF sample of a Pareto(alpha=1.5) capped
                        // so the worst draws stay finite. alpha=1.5 puts
                        // the tail squarely in the "heavy" regime - Hill
                        // index lands well above the exponential noise
                        // floor for n around 5k+.
                        let u = (rng.next_f64()).max(1e-9);
                        let mult = (1.0 / u).powf(1.0 / 1.5).min(60.0);
                        ((self.base_ns as f64) * mult) as u64
                    }
                }
            };
            out.push(v);
        }
        out
    }
}

/// Per-run analysis bundle. One of these per sample stream. Reads top-to
/// -bottom in the same order the recipe writeup introduces the analyses.
#[derive(Clone, Debug)]
pub struct StatsReport {
    pub count: usize,
    // Core percentiles / moments.
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
    pub mean_ns: u64,
    pub stddev_ns: u64,
    pub sweep: Vec<(f64, u64)>,
    // Tail.
    pub cte99_ns: u64,
    pub tail_fatness: f64,
    pub hill_50: Option<f64>,
    // Robust.
    pub iqr_ns: u64,
    pub mad_ns: u64,
    pub cov: f64,
    pub skew: f64,
    pub kurt: f64,
    // Jitter + histogram.
    pub jitter: f64,
    pub cdf_buckets: Vec<u64>,
    // Single-run bootstrap CI on p99 - useful even before a comparison
    // exists, because it tells you whether the reported p99 is a tight
    // point estimate or already too noisy to act on.
    pub p99_ci_ns: (u64, u64),
}

/// Two-run comparison bundle. Drives the `compare` and `bootstrap` modules
/// against a (baseline, candidate) pair.
#[derive(Clone, Debug)]
pub struct CompareReport {
    pub ks: Option<f64>,
    pub cohens_d: Option<f64>,
    pub baseline_p99_ci: (u64, u64),
    pub candidate_p99_ci: (u64, u64),
}

/// Default bootstrap parameters. Pulled into a const so the test asserting
/// determinism and the example can stay in lockstep.
pub const DEFAULT_BOOTSTRAP_ITERS: usize = 500;
pub const DEFAULT_BOOTSTRAP_CONFIDENCE: f64 = 0.95;
pub const DEFAULT_BOOTSTRAP_SEED: u64 = 0x00C0_FFEE;

/// Run every `subms-stats` analysis over a single sample stream.
///
/// The bootstrap CI uses [`DEFAULT_BOOTSTRAP_ITERS`] resamples at
/// [`DEFAULT_BOOTSTRAP_CONFIDENCE`]; the seed is derived from the caller's
/// seed so two streams in the same process don't collide on their LCG.
pub fn analyse(samples: &[u64]) -> StatsReport {
    let s = SubMsSamples::new(samples);
    StatsReport {
        count: s.count(),
        p50_ns: s.p50(),
        p90_ns: s.p90(),
        p99_ns: s.p99(),
        p999_ns: s.p999(),
        max_ns: s.max(),
        mean_ns: s.mean(),
        stddev_ns: s.stddev(),
        sweep: percentile_sweep(samples, 0.50, 0.999, 0.10),
        cte99_ns: conditional_tail_expectation(samples, 0.99),
        tail_fatness: tail_fatness_ratio(samples),
        hill_50: hill_tail_index(samples, 50),
        iqr_ns: iqr(samples),
        mad_ns: median_absolute_deviation(samples),
        cov: coefficient_of_variation(samples),
        skew: skewness(samples),
        kurt: kurtosis(samples),
        jitter: jitter_score(samples),
        cdf_buckets: cdf_buckets(samples),
        p99_ci_ns: bootstrap_percentile_ci(
            samples,
            0.99,
            DEFAULT_BOOTSTRAP_ITERS,
            DEFAULT_BOOTSTRAP_CONFIDENCE,
            DEFAULT_BOOTSTRAP_SEED,
        ),
    }
}

/// Run the two-run comparison: KS statistic + Cohen's d + bootstrap CIs on
/// both p99s. The CIs let a reader judge whether a KS verdict is supported
/// by non-overlapping intervals or just nudged into the threshold by noise.
pub fn compare(baseline: &[u64], candidate: &[u64]) -> CompareReport {
    CompareReport {
        ks: ks_statistic(baseline, candidate),
        cohens_d: cohens_d(baseline, candidate),
        baseline_p99_ci: bootstrap_percentile_ci(
            baseline,
            0.99,
            DEFAULT_BOOTSTRAP_ITERS,
            DEFAULT_BOOTSTRAP_CONFIDENCE,
            DEFAULT_BOOTSTRAP_SEED,
        ),
        candidate_p99_ci: bootstrap_percentile_ci(
            candidate,
            0.99,
            DEFAULT_BOOTSTRAP_ITERS,
            DEFAULT_BOOTSTRAP_CONFIDENCE,
            DEFAULT_BOOTSTRAP_SEED.wrapping_add(1),
        ),
    }
}

// --- internal -------------------------------------------------------------

// Stand-alone LCG so the example stays a single dep (subms-stats). Same
// constants the harness uses, so swapping in `SubMsLcg` later doesn't shift
// the synthetic data.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f64(&mut self) -> f64 {
        // Top 53 bits -> [0.0, 1.0). Standard trick.
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn bounded(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.next_u64() as u32) % n
    }
}
