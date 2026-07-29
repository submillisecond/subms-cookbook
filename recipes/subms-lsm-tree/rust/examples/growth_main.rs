//! Storage-growth capture for the LSM tree: overwrite the SAME key set every
//! round and watch on-disk bytes + SSTable count against a FLAT live set. Without
//! compaction each flush would leave a fresh run while the previous rounds' now-
//! dead versions pile up unreclaimed - the classic write-amplification leak. With
//! compaction ENABLED (`set_compaction_trigger`, plus a merge at each round
//! boundary), the tree reclaims those dead versions and on-disk stays within a
//! small multiple of the live set. This bench runs the fixed, compacting config
//! and gates the amplification bounded - green, where the un-compacted base tree
//! would breach.
//!
//! Emits the stable subms growth JSON on stdout.
//!
//! ```sh
//! cat <<EOF | cargo run --release --example growth_main --features harness
//! rounds=50
//! keys=2000
//! value_bytes=256
//! flush_threshold_bytes=65536
//! compaction_trigger=4
//! EOF
//! ```

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use subms::{SubMsGrowthClass, SubMsGrowthRecipe, grow, growth_to_json};
use subms_lsm_tree::LsmTree;

// With compaction the tree must keep on-disk within a small multiple of live
// data even under overwrite churn. A little slack over 1x for the runs that
// accumulate between compactions.
const AMPLIFICATION_CEILING: f64 = 3.0;

struct OverwriteChurnRecipe {
    lsm: LsmTree,
    dir: PathBuf,
    rounds: usize,
    keys: usize,
    value_bytes: usize,
    value: Vec<u8>,
}

impl OverwriteChurnRecipe {
    fn new(
        dir: PathBuf,
        rounds: usize,
        keys: usize,
        value_bytes: usize,
        flush_threshold: usize,
        compaction_trigger: usize,
    ) -> io::Result<Self> {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let mut lsm = LsmTree::open(&dir, flush_threshold)?;
        // The fix: auto-compact once runs accumulate, so overwritten versions are
        // reclaimed instead of piling up.
        lsm.set_compaction_trigger(compaction_trigger);
        Ok(Self {
            lsm,
            dir,
            rounds,
            keys,
            value_bytes,
            value: vec![b'x'; value_bytes],
        })
    }
    fn key(i: usize) -> String {
        format!("k{i:08}")
    }
}

impl SubMsGrowthRecipe for OverwriteChurnRecipe {
    fn name(&self) -> &str {
        "subms-lsm-tree"
    }
    fn op_name(&self) -> &str {
        "put"
    }
    fn rounds(&self) -> usize {
        self.rounds
    }
    fn ops_per_round(&self) -> usize {
        self.keys
    }
    fn op(&mut self, _round: usize, i: usize) {
        // Overwrite the SAME key each round: the live set is flat, so every byte
        // compaction fails to reclaim shows up as pure amplification.
        self.lsm.put(&Self::key(i), &self.value).expect("put");
    }
    fn end_round(&mut self, _round: usize) {
        // Flush the memtable, then compact so on-disk reflects only live data at
        // the round boundary - the reclaim the fix provides.
        self.lsm.flush().expect("flush");
        self.lsm.compact().expect("compact");
    }
    fn disk_bytes(&mut self) -> u64 {
        // Sum every file under the data dir (SSTables + their bloom trailers).
        std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter_map(|e| e.metadata().ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }
    fn live_bytes(&mut self) -> u64 {
        // The logical working set: `keys` distinct entries, each one value plus
        // its key. Flat across rounds because we only ever overwrite.
        (self.keys as u64) * (self.value_bytes as u64 + Self::key(0).len() as u64)
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("sstables".to_string(), self.lsm.sstable_count() as u64)]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        (
            SubMsGrowthClass::AmplificationBounded,
            AMPLIFICATION_CEILING,
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
    let keys = parse_usize(&map, "keys", 2000);
    let value_bytes = parse_usize(&map, "value_bytes", 256);
    let flush_threshold = parse_usize(&map, "flush_threshold_bytes", 64 * 1024);
    let compaction_trigger = parse_usize(&map, "compaction_trigger", 4);

    let dir = std::env::temp_dir().join(format!("lsm-growth-{}", std::process::id()));
    let mut recipe = match OverwriteChurnRecipe::new(
        dir,
        rounds,
        keys,
        value_bytes,
        flush_threshold,
        compaction_trigger,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("growth_main: setup failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let report = grow(&mut recipe, "rust");

    if growth_to_json(&report, &mut io::stdout().lock()).is_err() {
        eprintln!("growth_main: failed to write json");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
