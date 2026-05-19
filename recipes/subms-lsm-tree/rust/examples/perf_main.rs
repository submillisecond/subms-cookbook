//! Run the LSM-tree perf workload via the standard `subms` harness.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example perf_main --features harness
//! entries=50000
//! flush_threshold_bytes=16000
//! warmup=5000
//! bloom_mode=on
//! seed=0
//! EOF
//! ```

use std::io;
use std::process::ExitCode;

use subms::{SubMsBenchParams, benchmark, parse_bool, parse_usize, read_stdin_kv};
use subms_lsm_tree::BloomMode;
use subms_lsm_tree::recipe::LsmTreeRecipe;

fn main() -> ExitCode {
    let args = read_stdin_kv();
    let params = SubMsBenchParams::from_map(&args);
    let flush_threshold = parse_usize(&args, "flush_threshold_bytes", 16_000);
    let bloom_on = parse_bool(&args, "bloom_mode", true);
    let bloom_mode = if bloom_on {
        BloomMode::On
    } else {
        BloomMode::Off
    };

    let recipe = LsmTreeRecipe::new(flush_threshold, bloom_mode);
    let h = benchmark(&recipe, &params);
    h.write_json(&mut io::stdout().lock()).expect("write json");
    ExitCode::SUCCESS
}
