//! Sample app: a tour of `subms-stats`, base API first, then each optional
//! feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features tail,robust`) to see the
//! feature sections light up.
//!
//! The scenario is a batch of order-ack latencies (nanoseconds) captured off a
//! trading gateway. Every analysis reads the already-recorded sample array; the
//! recipe never touches the hot path that produced it.
//!
//! * base      - p50/p99/p999, mean, stddev over the ack-latency batch
//! * histogram - a log2-spaced CDF for exporting the whole distribution
//! * jitter    - was the measurement rig stable across the run?
//! * tail      - conditional tail expectation, Hill index, fatness ratio
//! * robust    - IQR, MAD, CoV, skewness, kurtosis (outlier-resistant spread)
//! * compare   - KS statistic + Cohen's d between a baseline and a candidate
//! * bootstrap - a confidence interval around the p99 point estimate

use subms_stats::SubMsSamples;

fn main() {
    let acks = ack_latencies_ns();
    base_summary(&acks);

    #[cfg(feature = "histogram")]
    histogram_cdf(&acks);

    #[cfg(feature = "jitter")]
    jitter_stability(&acks);

    #[cfg(feature = "tail")]
    tail_shape(&acks);

    #[cfg(feature = "robust")]
    robust_spread(&acks);

    #[cfg(feature = "compare")]
    compare_baseline_candidate();

    #[cfg(feature = "bootstrap")]
    bootstrap_p99_interval(&acks);
}

/// Base API: headline percentiles plus first-moment stats over the batch. This
/// is the always-on core - it needs no Cargo feature.
fn base_summary(acks: &[u64]) {
    println!(
        "== base: order-ack latency summary ({} samples) ==",
        acks.len()
    );
    let s = SubMsSamples::new(acks);
    println!("  p50 {} ns", s.p50());
    println!("  p99 {} ns", s.p99());
    println!("  p999 {} ns", s.p999());
    println!(
        "  mean {} ns  stddev {} ns  max {} ns",
        s.mean(),
        s.stddev(),
        s.max()
    );
    assert!(s.p99() >= s.p50(), "the tail never sits below the median");
    assert!(s.max() >= s.p999(), "max bounds every percentile");
}

/// `histogram` feature: a log2-spaced CDF of the batch. Downstream tooling
/// rebuilds any quantile from the bucket cumulative sums without shipping the
/// raw stream.
#[cfg(feature = "histogram")]
fn histogram_cdf(acks: &[u64]) {
    println!("\n== histogram: log2 CDF buckets ==");
    let s = SubMsSamples::new(acks);
    let buckets = s.cdf_buckets();
    let total: u64 = buckets.iter().sum();
    let modal = buckets
        .iter()
        .enumerate()
        .max_by_key(|&(_, &c)| c)
        .map(|(i, _)| i)
        .unwrap();
    println!("  {} buckets, {} samples total", buckets.len(), total);
    println!(
        "  modal bucket {} covers [2^{}, 2^{}) ns",
        modal,
        modal,
        modal + 1
    );
    assert_eq!(
        total as usize,
        acks.len(),
        "every sample lands in exactly one bucket"
    );
}

/// `jitter` feature: coefficient of variation across non-overlapping windows.
/// High jitter means the measurement environment moved under our feet (GC,
/// scheduler preemption), not that the gateway got slower.
#[cfg(feature = "jitter")]
fn jitter_stability(acks: &[u64]) {
    println!("\n== jitter: measurement-rig stability ==");
    let s = SubMsSamples::new(acks);
    let score = s.jitter_score();
    println!("  jitter score {:.4} (0.0 clean, 1.0 hostile)", score);
    assert!(
        (0.0..=1.0).contains(&score),
        "score stays in the unit interval"
    );
}

/// `tail` feature: what a lone p99 hides. Conditional tail expectation is the
/// mean of the worst cases; the fatness ratio and Hill index say how heavy the
/// tail is.
#[cfg(feature = "tail")]
fn tail_shape(acks: &[u64]) {
    println!("\n== tail: heavy-tail diagnostics ==");
    let s = SubMsSamples::new(acks);
    let cte99 = s.conditional_tail_expectation(0.99);
    let fatness = s.tail_fatness_ratio();
    println!("  CTE(0.99) {} ns  (mean of the worst 1%)", cte99);
    println!("  fatness p99/p50 {:.2}", fatness);
    if let Some(hill) = s.hill_tail_index(64) {
        println!("  Hill index (top 64) {:.3}", hill);
    }
    assert!(cte99 >= s.p99(), "the worst-1% mean sits at or above p99");
    assert!(
        fatness > 1.0,
        "the injected spikes make the tail fatter than uniform"
    );
}

/// `robust` feature: spread measures that shrug off outliers. IQR and MAD are
/// the outlier-resistant analogues of stddev; positive skew and excess kurtosis
/// confirm the right-heavy latency shape.
#[cfg(feature = "robust")]
fn robust_spread(acks: &[u64]) {
    println!("\n== robust: outlier-resistant spread ==");
    let s = SubMsSamples::new(acks);
    println!(
        "  IQR {} ns  MAD {} ns",
        s.iqr(),
        s.median_absolute_deviation()
    );
    println!("  CoV {:.4}", s.coefficient_of_variation());
    println!(
        "  skewness {:.3}  excess kurtosis {:.3}",
        s.skewness(),
        s.kurtosis()
    );
    assert!(s.iqr() > 0, "the body of the distribution has real spread");
    assert!(s.skewness() > 0.0, "latency skews right");
}

/// `compare` feature: did a deploy shift the distribution? KS is the max CDF
/// gap; Cohen's d is the standardised mean shift. A slower candidate produces a
/// positive Cohen's d.
#[cfg(feature = "compare")]
fn compare_baseline_candidate() {
    use subms_stats::{cohens_d, ks_statistic};
    println!("\n== compare: baseline vs candidate deploy ==");
    let baseline = ack_latencies_ns();
    let candidate: Vec<u64> = baseline.iter().map(|&v| v + 120).collect();
    let ks = ks_statistic(&baseline, &candidate).unwrap();
    let d = cohens_d(&baseline, &candidate).unwrap();
    println!("  KS statistic {:.3}", ks);
    println!("  Cohen's d {:.3} (candidate is +120 ns slower)", d);
    assert!(d > 0.0, "the uniformly-slower candidate lifts the mean");
}

/// `bootstrap` feature: how wide is the p99 estimate? A deterministic LCG makes
/// the interval reproducible across runs given the same seed.
#[cfg(feature = "bootstrap")]
fn bootstrap_p99_interval(acks: &[u64]) {
    println!("\n== bootstrap: confidence interval around p99 ==");
    let s = SubMsSamples::new(acks);
    let (lo, hi) = s.bootstrap_percentile_ci(0.99, 500, 0.95, 42);
    println!("  p99 point {} ns", s.p99());
    println!("  95% CI [{} ns, {} ns]", lo, hi);
    assert!(lo <= hi, "the interval is ordered");
    assert!(
        lo <= s.p99() && s.p99() <= hi,
        "the point estimate sits inside its CI"
    );
}

/// A deterministic right-skewed ack-latency batch: a tight body around 800 ns
/// with periodic spikes near 5 us, so the tail and robust sections have real
/// shape to report. No RNG crate - a small LCG keeps it reproducible.
fn ack_latencies_ns() -> Vec<u64> {
    let mut out = Vec::with_capacity(2_048);
    let mut state: u64 = 0x9E3779B97F4A7C15;
    for i in 0..2_048u64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let body = 760 + (state >> 40) % 120; // 760..880 ns
        if i % 97 == 0 {
            out.push(4_800 + (state >> 32) % 600); // periodic tail spike
        } else {
            out.push(body);
        }
    }
    out
}
