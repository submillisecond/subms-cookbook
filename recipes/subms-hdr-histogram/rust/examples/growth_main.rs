//! Storage-growth capture for the HDR histogram: record an unbounded stream of
//! values and confirm the footprint does not move. The counter array is sized
//! once at construction from the significant-digits setting (a fixed number of
//! sub-buckets x major buckets); recording only increments existing counters, so
//! memory is O(1) in the number of samples. This is a compact verdict, not a
//! curve - there is nothing to grow.
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

use subms::{SubMsGrowthClass, SubMsGrowthRecipe, grow, growth_to_json};
use subms_hdr_histogram::HdrHistogram;

// A 3-significant-digit histogram is ~17k u64 counters (see the recipe docs:
// 2^9 sub-buckets over ~33 major buckets). The array is allocated once and never
// grows, so this is the fixed footprint whatever the sample count.
fn bucket_bytes(sig_digits: u32) -> u64 {
    // ~17k counters at 3 digits; each extra digit ~10x the sub-buckets.
    let counters = 17_000u64 * 10u64.pow(sig_digits.saturating_sub(3));
    counters * 8
}

struct HdrRecord {
    hist: HdrHistogram,
    rounds: usize,
    records_per_round: usize,
    footprint: u64,
    lcg: u64,
}

impl SubMsGrowthRecipe for HdrRecord {
    fn name(&self) -> &str {
        "subms-hdr-histogram"
    }
    fn op_name(&self) -> &str {
        "record"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.records_per_round
    }
    fn op(&mut self, _round: usize, _i: usize) {
        // A spread of values across the whole range, so every major bucket is
        // touched - still O(1) memory, the counters already exist.
        self.lcg = self.lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
        let v = (self.lcg >> 33) % 1_000_000_000;
        self.hist.record(v.max(1));
    }
    fn memory_bytes(&mut self) -> u64 {
        self.footprint
    }
    fn live_bytes(&mut self) -> u64 {
        self.footprint
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("records".to_string(), self.hist.count())]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        (SubMsGrowthClass::Bounded, self.footprint as f64 * 1.01)
    }
    fn compact(&self) -> bool {
        true
    }
}

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
    let rounds = parse_usize(&map, "rounds", 20);
    let sig_digits = parse_usize(&map, "significant_digits", 3) as u32;
    let records_per_round = parse_usize(&map, "records_per_round", 50_000);

    let mut recipe = HdrRecord {
        hist: HdrHistogram::new(sig_digits),
        rounds,
        records_per_round,
        footprint: bucket_bytes(sig_digits),
        lcg: 0x1234_5678,
    };
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
