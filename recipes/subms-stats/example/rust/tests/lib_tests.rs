//! Tests for the subms-stats recipe example surface: workload determinism,
//! every wrapper analysis lands in a sensible range, and the two-run
//! comparison detects the synthetic power-law regression.

use subms_stats_example::{
    DEFAULT_BOOTSTRAP_ITERS, StatsReport, TailShape, Workload, analyse, compare,
};

fn baseline_workload() -> Workload {
    Workload {
        n: 5_000,
        base_ns: 800,
        shape: TailShape::Uniformish,
        seed: 0xBA5E_BA5E_BA5E_BA5E,
    }
}

fn candidate_workload() -> Workload {
    Workload {
        n: 5_000,
        base_ns: 850,
        shape: TailShape::PowerLaw,
        seed: 0xCADD_A7E0_CADD_A7E0,
    }
}

#[test]
fn workload_is_deterministic_under_same_seed() {
    let a = baseline_workload().generate();
    let b = baseline_workload().generate();
    assert_eq!(a, b, "same seed must produce byte-identical samples");
}

#[test]
fn workload_differs_under_different_seeds() {
    let mut w = baseline_workload();
    let a = w.generate();
    // Bump by 2 - the LCG sets the low bit, so +1 collides with the original
    // seed after the `| 1` mask.
    w.seed = w.seed.wrapping_add(2);
    let b = w.generate();
    assert_ne!(a, b);
    assert_eq!(a.len(), b.len());
}

#[test]
fn uniformish_workload_has_modest_tail() {
    let raw = baseline_workload().generate();
    let r = analyse(&raw);
    assert_eq!(r.count, 5_000);

    // p50 is the body; p99 sits inside the rare-outlier region (~2-3x base).
    assert!(r.p50_ns > 0);
    assert!(
        r.p99_ns > r.p50_ns,
        "p99 should exceed p50 ({} vs {})",
        r.p99_ns,
        r.p50_ns,
    );
    assert!(
        r.tail_fatness < 4.0,
        "uniformish workload should not produce a heavy tail: fatness = {}",
        r.tail_fatness,
    );
}

#[test]
fn power_law_workload_has_heavier_tail_than_uniformish() {
    let uniformish = analyse(&baseline_workload().generate());
    let powerlaw = analyse(&candidate_workload().generate());

    // The synthetic regression is exactly this: the candidate's tail is
    // power-law, the baseline's is bounded. We expect every tail-shape
    // indicator to move in the same direction.
    assert!(
        powerlaw.tail_fatness > uniformish.tail_fatness,
        "power-law fatness {} should exceed uniformish {}",
        powerlaw.tail_fatness,
        uniformish.tail_fatness,
    );
    assert!(
        powerlaw.p999_ns > uniformish.p999_ns,
        "power-law p99.9 {} should exceed uniformish p99.9 {}",
        powerlaw.p999_ns,
        uniformish.p999_ns,
    );
    if let (Some(p), Some(u)) = (powerlaw.hill_50, uniformish.hill_50) {
        assert!(
            p > u,
            "power-law Hill index {p} should exceed uniformish {u}",
        );
    }
}

#[test]
fn robust_stats_sit_in_their_documented_ranges() {
    let r = analyse(&baseline_workload().generate());

    assert!(r.iqr_ns > 0);
    assert!(r.mad_ns > 0);
    assert!(
        r.cov >= 0.0 && r.cov < 5.0,
        "CoV {} should be a small positive ratio for a tight workload",
        r.cov,
    );
    assert!(
        r.skew > 0.0,
        "right-tailed latency distribution should have positive skew: {}",
        r.skew,
    );
}

#[test]
fn jitter_score_lands_in_unit_interval() {
    let r = analyse(&baseline_workload().generate());
    assert!(
        (0.0..=1.0).contains(&r.jitter),
        "jitter score out of [0,1]: {}",
        r.jitter,
    );
}

#[test]
fn cdf_buckets_account_for_every_sample() {
    let r = analyse(&baseline_workload().generate());
    let total: u64 = r.cdf_buckets.iter().sum();
    assert_eq!(total as usize, r.count);
    assert_eq!(r.cdf_buckets.len(), 64);
}

#[test]
fn bootstrap_ci_brackets_point_p99() {
    let r = analyse(&baseline_workload().generate());
    assert!(
        r.p99_ci_ns.0 <= r.p99_ns && r.p99_ns <= r.p99_ci_ns.1,
        "p99 {} must fall inside its bootstrap CI [{}, {}]",
        r.p99_ns,
        r.p99_ci_ns.0,
        r.p99_ci_ns.1,
    );
    // CI width is bounded but non-degenerate at 5000 samples + 500 iters.
    let width = r.p99_ci_ns.1.saturating_sub(r.p99_ci_ns.0);
    assert!(
        width > 0,
        "CI width should be > 0 at iters={DEFAULT_BOOTSTRAP_ITERS}"
    );
}

#[test]
fn analyse_is_deterministic_given_the_same_samples() {
    let raw = baseline_workload().generate();
    let a = analyse(&raw);
    let b = analyse(&raw);
    assert_eq!(a.p99_ns, b.p99_ns);
    assert_eq!(a.p99_ci_ns, b.p99_ci_ns, "bootstrap seed should be fixed");
    assert_eq!(a.cdf_buckets, b.cdf_buckets);
    assert_field_floats_match(&a, &b);
}

#[test]
fn comparison_flags_the_synthetic_regression() {
    let baseline = baseline_workload().generate();
    let candidate = candidate_workload().generate();
    let cmp = compare(&baseline, &candidate);

    let ks = cmp.ks.expect("KS available for two non-empty samples");
    let d = cmp
        .cohens_d
        .expect("Cohen's d available for two non-empty samples");

    // The candidate has a power-law tail the baseline doesn't, so KS must
    // pick up a meaningful CDF gap and Cohen's d must come out positive
    // (candidate is slower on average).
    assert!(ks > 0.05, "KS {ks} should detect the synthetic regression",);
    assert!(
        d > 0.05,
        "Cohen's d {d} should be positive when candidate is slower",
    );
    // CIs are non-degenerate.
    assert!(cmp.baseline_p99_ci.1 >= cmp.baseline_p99_ci.0);
    assert!(cmp.candidate_p99_ci.1 >= cmp.candidate_p99_ci.0);
}

// Smaller harness for the deterministic-analyse test: spot-check the f64
// fields that f64 == doesn't read well on.
fn assert_field_floats_match(a: &StatsReport, b: &StatsReport) {
    assert!((a.tail_fatness - b.tail_fatness).abs() < 1e-9);
    assert!((a.cov - b.cov).abs() < 1e-9);
    assert!((a.skew - b.skew).abs() < 1e-9);
    assert!((a.kurt - b.kurt).abs() < 1e-9);
    assert!((a.jitter - b.jitter).abs() < 1e-9);
    match (a.hill_50, b.hill_50) {
        (Some(x), Some(y)) => assert!((x - y).abs() < 1e-9),
        (None, None) => {}
        _ => panic!("Hill index Option<f64> differed between runs"),
    }
}
