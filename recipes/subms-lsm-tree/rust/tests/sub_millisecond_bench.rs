//! Runs the LSM recipe under both bloom modes back-to-back, prints the shared
//! harness percentile table, then asserts p99 < 1 ms for the {bloom = on} pass.
//!
//! The same code in Java lives at
//! `recipes/subms-lsm-tree/java/.../SubmillisecondBench.java`; output is
//! byte-equivalent modulo per-run jitter.
//!
//! ```sh
//! cargo test --release --features harness --test sub_millisecond_bench -- --nocapture
//! ```

#![cfg(feature = "harness")]

use std::io;

use subms::{
    SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, print_summary, run_bench,
    summarize_lean,
};
use subms_lsm_tree::BloomMode;
use subms_lsm_tree::recipe::LsmTreeRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 50_000,
        warmup: 5_000,
        seed: 0,
        ..Default::default()
    };

    let on = summarize_lean(&run_bench(
        &LsmTreeRecipe::new(16_000, BloomMode::On),
        &params,
    ));
    let off = summarize_lean(&run_bench(
        &LsmTreeRecipe::new(16_000, BloomMode::Off),
        &params,
    ));

    let mut out = io::stdout().lock();
    use io::Write as _;
    writeln!(
        out,
        "entries={}  flush_threshold_bytes={}  warmup={}\n",
        params.entries, 16_000, params.warmup
    )
    .unwrap();
    writeln!(out, "bloom = on").unwrap();
    print_summary(&on, &mut out).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "bloom = off").unwrap();
    print_summary(&off, &mut out).unwrap();

    assert_p99_under(
        &on,
        &[
            SubMsBenchAssertion {
                stage: "put",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "get_hit",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "get_miss",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();

    writeln!(out, "\nOK (BloomMode::On - all p99 < 1ms)").unwrap();
}
