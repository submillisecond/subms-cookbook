//! Storage-growth capture for the bump arena: run many alloc-then-`reset`
//! sessions and confirm resident memory returns to the same steady level every
//! round. A bump allocator has zero per-object overhead, so resident tracks the
//! live allocations exactly (amplification ~1x), and `reset` reclaims all of it -
//! the arena never accretes across sessions.
//!
//! Emits the stable subms growth JSON on stdout.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example growth_main --features harness
//! rounds=50
//! allocs_per_round=20000
//! EOF
//! ```

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::process::ExitCode;

use subms::{SubMsGrowthClass, SubMsGrowthRecipe, grow, growth_to_json};
use subms_arena_allocator::Bump;

struct ArenaChurn {
    arena: Bump,
    rounds: usize,
    allocs_per_round: usize,
}

impl SubMsGrowthRecipe for ArenaChurn {
    fn name(&self) -> &str {
        "subms-arena-allocator"
    }
    fn op_name(&self) -> &str {
        "alloc"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.allocs_per_round
    }
    fn op(&mut self, _round: usize, i: usize) {
        // Start each round's session fresh: reset reclaims the whole buffer, then
        // we bump-allocate the round's objects into it.
        if i == 0 {
            self.arena.reset();
        }
        let _ = self.arena.alloc_copy(i as u64);
    }
    fn memory_bytes(&mut self) -> u64 {
        // Resident = bytes currently handed out of the buffer (measured at the
        // round's peak, before the next round's reset).
        self.arena.used() as u64
    }
    fn live_bytes(&mut self) -> u64 {
        // Every allocated byte is live until reset, so resident == live: a bump
        // arena wastes nothing (amplification 1x).
        self.arena.used() as u64
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("live_allocs".to_string(), self.allocs_per_round as u64)]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        // Resident memory must return to the same steady level every round - it
        // must not climb, which would mean reset is leaking the buffer.
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
    let allocs_per_round = parse_usize(&map, "allocs_per_round", 20_000);

    // Buffer sized to hold one round's allocations (u64 + alignment) with headroom
    // - the base Bump is fixed-capacity and panics rather than growing.
    let capacity = allocs_per_round * 16 + 4096;
    let mut recipe = ArenaChurn {
        arena: Bump::with_capacity(capacity),
        rounds,
        allocs_per_round,
    };
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
