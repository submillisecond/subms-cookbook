//! Storage-growth capture for the block cache: stream far more distinct keys
//! through a fixed-capacity LRU than it can hold, and confirm the resident set
//! stays pinned at the capacity. Eviction is what bounds memory here - this
//! proves it, rather than the cache being a map that quietly grows.
//!
//! Emits the stable subms growth JSON on stdout.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example growth_main --features harness
//! rounds=50
//! capacity=1024
//! inserts_per_round=20000
//! EOF
//! ```

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::process::ExitCode;

use subms::{SubMsGrowthClass, SubMsGrowthRecipe, grow, growth_to_json};
use subms_block_cache::BlockCache;

const VALUE_BYTES: usize = 256;
// Rough per-entry heap cost: value + key (u64) + slot/index bookkeeping.
const ENTRY_BYTES: u64 = (VALUE_BYTES + 8 + 32) as u64;

struct CacheChurn {
    cache: BlockCache<u64, Vec<u8>>,
    capacity: usize,
    rounds: usize,
    inserts_per_round: usize,
    value: Vec<u8>,
    next: u64,
}

impl SubMsGrowthRecipe for CacheChurn {
    fn name(&self) -> &str {
        "subms-block-cache"
    }
    fn op_name(&self) -> &str {
        "put"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.inserts_per_round
    }
    fn op(&mut self, _round: usize, _i: usize) {
        // A fresh distinct key every op: nothing is ever re-hit, so a naive
        // unbounded map would grow to millions - the LRU must evict to stay flat.
        self.cache.put(self.next, self.value.clone());
        self.next += 1;
    }
    fn memory_bytes(&mut self) -> u64 {
        self.cache.len() as u64 * ENTRY_BYTES
    }
    fn live_bytes(&mut self) -> u64 {
        // A cache holds only live entries, so resident == live: amplification 1x
        // is the healthy shape (every resident byte is one the cache chose to keep).
        self.cache.len() as u64 * ENTRY_BYTES
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("entries".to_string(), self.cache.len() as u64)]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        // Resident memory must never exceed the capacity's worth, no matter how
        // many distinct keys stream through. 5% slack for bookkeeping estimate.
        (
            SubMsGrowthClass::Bounded,
            (self.capacity as u64 * ENTRY_BYTES) as f64 * 1.05,
        )
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
    let capacity = parse_usize(&map, "capacity", 1024);
    let inserts_per_round = parse_usize(&map, "inserts_per_round", 20_000);

    let mut recipe = CacheChurn {
        cache: BlockCache::with_capacity(capacity),
        capacity,
        rounds,
        inserts_per_round,
        value: vec![0u8; VALUE_BYTES],
        next: 0,
    };
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
