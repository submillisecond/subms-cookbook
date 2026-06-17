//! End-to-end driver for the perf-harness primer. Reads stdin params (with
//! sensible defaults), runs the recipe through `subms::run_bench`, prints the
//! percentile table to stderr, asserts the sub-millisecond gate, and emits
//! the canonical JSON summary to stdout.
//!
//! ```sh
//! cargo run --release --example perf_main > perf/rust.json
//!
//! # or with custom params:
//! cat <<EOF | cargo run --release --example perf_main > perf/rust.json
//! entries=50000
//! warmup=5000
//! seed=0
//! EOF
//! ```
//!
//! The split between stdout and stderr matters: stdout is the JSON capture
//! path, stderr is human-readable run context. Piping to a file leaves you
//! with a valid `perf/rust.json`.

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use subms::{
    SubMsBenchParams, assert_p99_under, print_summary, run_bench, summarize, summary_to_json,
};
use subms_primer_perf_harness::{HarnessRecipe, default_assertions};

fn main() -> ExitCode {
    let params = if io::stdin().is_terminal() {
        SubMsBenchParams::default()
    } else {
        SubMsBenchParams::from_stdin()
    };

    let mut stderr = io::stderr().lock();
    writeln!(
        stderr,
        "tinymap: entries={} warmup={} seed={}",
        params.entries, params.warmup, params.seed
    )
    .ok();

    let harness = run_bench(&HarnessRecipe, &params);
    let summary = summarize(&harness);

    print_summary(&summary, &mut stderr).expect("print summary");

    if let Err(e) = assert_p99_under(&summary, &default_assertions()) {
        writeln!(stderr, "sub-millisecond gate FAILED: {e}").ok();
        // Still emit the JSON so the failing run is captured for review.
        summary_to_json(&summary, &mut io::stdout().lock()).expect("write json");
        return ExitCode::FAILURE;
    }

    summary_to_json(&summary, &mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
