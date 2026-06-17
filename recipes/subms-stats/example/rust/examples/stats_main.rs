//! End-to-end driver for the `subms-stats` recipe example. Builds two
//! synthetic latency-sample streams - a "baseline" with bounded outliers and
//! a "candidate" with a deliberate power-law tail regression - and prints a
//! structured analysis for each, followed by a two-run comparison block.
//!
//! ```sh
//! cargo run --release --example stats_main
//! ```
//!
//! Output is plain text. No machine-readable JSON is emitted; that's the
//! job of a recipe's `perf_main`, not this example.

use subms_stats_example::{StatsReport, TailShape, Workload, analyse, compare};

fn main() {
    let baseline_wl = Workload {
        n: 5_000,
        base_ns: 800,
        shape: TailShape::Uniformish,
        seed: 0xBA5E_BA5E_BA5E_BA5E,
    };
    let candidate_wl = Workload {
        n: 5_000,
        base_ns: 850,
        shape: TailShape::PowerLaw,
        seed: 0xCADD_A7E0_CADD_A7E0,
    };

    let baseline = baseline_wl.generate();
    let candidate = candidate_wl.generate();

    print_workload("baseline", &baseline_wl);
    let baseline_report = analyse(&baseline);
    print_report(&baseline_report);

    print_workload("candidate (slight regression)", &candidate_wl);
    let candidate_report = analyse(&candidate);
    print_report(&candidate_report);

    println!();
    println!("=== Compare (baseline vs candidate) ===");
    let cmp = compare(&baseline, &candidate);
    let ks = cmp.ks.map(|v| format!("{v:.4}")).unwrap_or_else(empty);
    let d = cmp
        .cohens_d
        .map(|v| format!("{v:+.4}"))
        .unwrap_or_else(empty);
    println!("  KS statistic           : {ks}");
    println!("  Cohen's d              : {d}");
    println!(
        "  baseline  p99 95% CI   : [{}, {}] ns",
        cmp.baseline_p99_ci.0, cmp.baseline_p99_ci.1
    );
    println!(
        "  candidate p99 95% CI   : [{}, {}] ns",
        cmp.candidate_p99_ci.0, cmp.candidate_p99_ci.1
    );

    // Editorial verdict. Threshold values track the conventional Cohen's d
    // ranges (small / medium / large) and a KS gap that won't fire on
    // run-to-run noise at n=5k.
    let verdict = match (cmp.ks, cmp.cohens_d) {
        (Some(ks), Some(d)) if ks > 0.10 && d.abs() > 0.20 => {
            "distribution shift detected (KS clears 0.10, |d| clears 0.20)"
        }
        (Some(_), Some(_)) => "no clear shift - changes within noise",
        _ => "comparison unavailable (empty input?)",
    };
    println!("  verdict                : {verdict}");
}

fn print_workload(label: &str, w: &Workload) {
    println!();
    println!("=== Run: {label} ===");
    println!(
        "  workload               : n={} base_ns={} shape={:?} seed={:#x}",
        w.n, w.base_ns, w.shape, w.seed
    );
}

fn print_report(r: &StatsReport) {
    println!("  count                  : {}", r.count);
    println!(
        "  p50 / p90 / p99 / p99.9 : {} / {} / {} / {} ns",
        r.p50_ns, r.p90_ns, r.p99_ns, r.p999_ns
    );
    println!("  max                    : {} ns", r.max_ns);
    println!(
        "  mean / stddev          : {} / {} ns",
        r.mean_ns, r.stddev_ns
    );
    println!(
        "  p99 95% CI (bootstrap) : [{}, {}] ns",
        r.p99_ci_ns.0, r.p99_ci_ns.1
    );
    println!(
        "  CTE@p99                : {} ns   (mean of samples above p99)",
        r.cte99_ns
    );
    println!("  tail fatness (p99/p50) : {:.3}", r.tail_fatness);
    match r.hill_50 {
        Some(h) => println!("  Hill index (k=50)      : {h:+.3}"),
        None => println!("  Hill index (k=50)      : <too few samples>"),
    }
    println!("  IQR / MAD              : {} / {} ns", r.iqr_ns, r.mad_ns);
    println!(
        "  CoV / skew / kurt      : {:.3} / {:+.3} / {:+.3}",
        r.cov, r.skew, r.kurt
    );
    println!("  jitter (0=clean,1=loud): {:.3}", r.jitter);

    // Percentile sweep - one quick scan from p50 to p999 in 10-percentile
    // steps, so the body and the climb to the tail land on the same line.
    let sweep: String = r
        .sweep
        .iter()
        .map(|(q, ns)| format!("p{:>5.1}={}ns", q * 100.0, ns))
        .collect::<Vec<_>>()
        .join("  ");
    println!("  sweep                  : {sweep}");

    // CDF buckets: log2-spaced. Print only the non-zero stretch so the
    // output stays readable.
    let first = r.cdf_buckets.iter().position(|&c| c > 0).unwrap_or(0);
    let last = r.cdf_buckets.iter().rposition(|&c| c > 0).unwrap_or(first);
    let mut hist = String::new();
    for (i, &c) in r.cdf_buckets[first..=last].iter().enumerate() {
        let bucket = first + i;
        let lo = if bucket == 0 { 0 } else { 1u64 << bucket };
        let hi = 1u64 << (bucket + 1);
        hist.push_str(&format!("  [{lo}..{hi}):{c}"));
    }
    println!("  CDF buckets            :{hist}");
}

fn empty() -> String {
    "<n/a>".to_string()
}
