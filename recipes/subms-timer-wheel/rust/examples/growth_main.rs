//! Storage-growth capture for the timer wheel: under continuous schedule+tick
//! churn, the pending-timer index (`id_to_slot`) must reach a bounded steady
//! state and stay there. If `tick` fired timers but forgot to evict their ids,
//! the index would climb every round - a slow leak a percentile can't see. This
//! measures the index size round over round and gates it flat.
//!
//! Emits the stable subms growth JSON on stdout.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example growth_main --features harness
//! rounds=50
//! num_slots=256
//! ops_per_round=20000
//! EOF
//! ```

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::process::ExitCode;

use subms::{SubMsGrowthClass, SubMsGrowthRecipe, grow, growth_to_json};
use subms_timer_wheel::TimerWheel;

// Rough per-pending-entry heap cost: the id->slot map entry plus the slot's
// Entry<u64> (id + rounds + value + flag).
const ENTRY_BYTES: u64 = 48;

struct WheelChurn {
    wheel: TimerWheel<u64>,
    num_slots: usize,
    rounds: usize,
    ops_per_round: usize,
    seq: u64,
}

impl SubMsGrowthRecipe for WheelChurn {
    fn name(&self) -> &str {
        "subms-timer-wheel"
    }
    fn op_name(&self) -> &str {
        "schedule"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.ops_per_round
    }
    fn op(&mut self, _round: usize, i: usize) {
        // Schedule one timer somewhere in the next rotation, then advance the hand
        // one tick (firing anything now due). Over many ops the in-flight set
        // reaches a steady size; a leaking tick would let it grow without bound.
        let delay = (i % (self.num_slots - 1)) + 1;
        self.wheel.schedule(delay, self.seq);
        self.seq += 1;
        let _ = self.wheel.tick();
    }
    fn memory_bytes(&mut self) -> u64 {
        self.wheel.pending() as u64 * ENTRY_BYTES
    }
    fn live_bytes(&mut self) -> u64 {
        // The genuinely-in-flight timers are the live set; a correct wheel holds
        // exactly those, so resident == live.
        self.wheel.pending() as u64 * ENTRY_BYTES
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("pending".to_string(), self.wheel.pending() as u64)]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        // The pending index must plateau at its steady size, not climb round over
        // round - climbing would mean fired ids are never evicted.
        (SubMsGrowthClass::PlateauBounded, 1.5)
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
    let rounds = parse_usize(&map, "rounds", 50);
    let num_slots = parse_usize(&map, "num_slots", 256).max(4);
    let ops_per_round = parse_usize(&map, "ops_per_round", 20_000);

    let mut recipe = WheelChurn {
        wheel: TimerWheel::new(num_slots),
        num_slots,
        rounds,
        ops_per_round,
        seq: 0,
    };
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
