//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base summary is internally consistent, and each feature reports the
//! shape of the injected right-skewed latency batch.

use subms_stats::SubMsSamples;

/// Mirror of the example's deterministic generator so the tests pin the same
/// data the sample narrates.
fn ack_latencies_ns() -> Vec<u64> {
    let mut out = Vec::with_capacity(2_048);
    let mut state: u64 = 0x9E3779B97F4A7C15;
    for i in 0..2_048u64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let body = 760 + (state >> 40) % 120;
        if i % 97 == 0 {
            out.push(4_800 + (state >> 32) % 600);
        } else {
            out.push(body);
        }
    }
    out
}

#[test]
fn base_summary_is_consistent() {
    let acks = ack_latencies_ns();
    let s = SubMsSamples::new(&acks);
    assert_eq!(s.count(), 2_048);
    assert!(s.p50() <= s.p99(), "median under the tail");
    assert!(s.p99() <= s.p999(), "p99 under p999");
    assert!(s.p999() <= s.max(), "max bounds every percentile");
    assert!(s.mean() > 0, "a non-empty batch has a positive mean");
}

#[cfg(feature = "histogram")]
#[test]
fn histogram_covers_every_sample() {
    let acks = ack_latencies_ns();
    let buckets = SubMsSamples::new(&acks).cdf_buckets();
    assert_eq!(buckets.len(), 64);
    let total: u64 = buckets.iter().sum();
    assert_eq!(
        total as usize,
        acks.len(),
        "no sample is lost or double counted"
    );
}

#[cfg(feature = "jitter")]
#[test]
fn jitter_score_in_unit_interval() {
    let acks = ack_latencies_ns();
    let score = SubMsSamples::new(&acks).jitter_score();
    assert!((0.0..=1.0).contains(&score), "score clamps to [0, 1]");
}

#[cfg(feature = "tail")]
#[test]
fn tail_reflects_injected_spikes() {
    let acks = ack_latencies_ns();
    let s = SubMsSamples::new(&acks);
    assert!(
        s.conditional_tail_expectation(0.99) >= s.p99(),
        "worst-1% mean >= p99"
    );
    assert!(
        s.tail_fatness_ratio() > 1.0,
        "spikes make the tail fatter than uniform"
    );
}

#[cfg(feature = "robust")]
#[test]
fn robust_spread_shows_right_skew() {
    let acks = ack_latencies_ns();
    let s = SubMsSamples::new(&acks);
    assert!(s.iqr() > 0, "the body has spread");
    assert!(s.skewness() > 0.0, "latency skews right");
}

#[cfg(feature = "compare")]
#[test]
fn compare_flags_a_slower_candidate() {
    use subms_stats::{cohens_d, ks_statistic};
    let baseline = ack_latencies_ns();
    let candidate: Vec<u64> = baseline.iter().map(|&v| v + 120).collect();
    assert!(
        ks_statistic(&baseline, &candidate).unwrap() > 0.0,
        "the CDFs differ"
    );
    assert!(
        cohens_d(&baseline, &candidate).unwrap() > 0.0,
        "the candidate is slower"
    );
}

#[cfg(feature = "bootstrap")]
#[test]
fn bootstrap_ci_brackets_the_point_estimate() {
    let acks = ack_latencies_ns();
    let s = SubMsSamples::new(&acks);
    let (lo, hi) = s.bootstrap_percentile_ci(0.99, 500, 0.95, 42);
    assert!(lo <= hi, "ordered interval");
    assert!(
        lo <= s.p99() && s.p99() <= hi,
        "point estimate inside its CI"
    );
}
