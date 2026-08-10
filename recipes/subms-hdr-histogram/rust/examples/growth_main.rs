//! Storage-growth capture for the HDR histogram: record an unbounded stream of
//! values and confirm the footprint does not move. The counter array is sized by
//! the largest value recorded, never by how many values were recorded, so memory
//! is O(1) in the sample count. This is a compact verdict, not a curve.
//!
//! Emits the stable subms growth JSON on stdout.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example growth_main --features harness
//! rounds=20
//! significant_digits=3
//! records_per_round=50000
//! EOF
//! ```

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::process::ExitCode;

use subms::{grow, growth_to_json};
use subms_hdr_histogram::growth::HdrGrowthRecipe;

fn parse_usize(map: &BTreeMap<String, String>, key: &str, default: usize) -> usize {
    map.get(key)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn main() -> ExitCode {
    let mut raw = String::new();
    if io::stdin().read_to_string(&mut raw).is_err() {
        eprintln!("growth_main: failed to read stdin");
        return ExitCode::FAILURE;
    }
    let mut map = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let mut recipe = HdrGrowthRecipe::new(
        parse_usize(&map, "significant_digits", 3) as u32,
        parse_usize(&map, "rounds", 20),
        parse_usize(&map, "records_per_round", 50_000),
    );
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
