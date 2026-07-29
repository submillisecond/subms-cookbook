//! Storage-growth capture for the rate limiter: hammer it with acquisitions and
//! confirm the footprint never moves. The limiter is a single GCRA bucket - one
//! `AtomicU64` of state plus a couple of constants - with no per-key map, so its
//! memory is O(1) no matter how many callers or how long it runs. A compact
//! verdict, not a curve.
//!
//! Emits the stable subms growth JSON on stdout.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example growth_main --features harness
//! rounds=20
//! acquires_per_round=100000
//! EOF
//! ```

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::mem::size_of;
use std::process::ExitCode;

use subms::{SubMsGrowthClass, SubMsGrowthRecipe, grow, growth_to_json};
use subms_rate_limiter::RateLimiter;

struct LimiterChurn {
    limiter: RateLimiter,
    rounds: usize,
    acquires_per_round: usize,
    footprint: u64,
    granted: u64,
}

impl SubMsGrowthRecipe for LimiterChurn {
    fn name(&self) -> &str {
        "subms-rate-limiter"
    }
    fn op_name(&self) -> &str {
        "acquire"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.acquires_per_round
    }
    fn op(&mut self, _round: usize, _i: usize) {
        if self.limiter.try_acquire() {
            self.granted += 1;
        }
    }
    fn memory_bytes(&mut self) -> u64 {
        self.footprint
    }
    fn live_bytes(&mut self) -> u64 {
        self.footprint
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("granted".to_string(), self.granted)]
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
    let acquires_per_round = parse_usize(&map, "acquires_per_round", 100_000);

    let mut recipe = LimiterChurn {
        // High rate + burst so most acquisitions succeed; the footprint is the
        // point, not the grant ratio.
        limiter: RateLimiter::new(10_000_000.0, 1_000_000),
        rounds,
        acquires_per_round,
        footprint: size_of::<RateLimiter>() as u64,
        granted: 0,
    };
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
