//! Adapter that runs the bloom-filter perf workload via the standard `subms` harness.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example perf_main --features harness
//! entries=50000
//! warmup=5000
//! seed=0
//! EOF
//! ```

use std::io;
use std::process::ExitCode;

use subms::{benchmark, SubMsBenchParams};
use subms_bloom_filter::recipe::BloomFilterRecipe;

fn main() -> ExitCode {
    let params = SubMsBenchParams::from_stdin();
    let h = benchmark(&BloomFilterRecipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
