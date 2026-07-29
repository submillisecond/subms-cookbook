//! Size-tiered compaction: when level L holds N runs of similar size, merge
//! them into a single larger run at level L+1.
//!
//! Tradeoff vs leveled: tiered keeps write amplification low (a record is
//! rewritten roughly `log_N(total)` times across its lifetime) but space
//! and read amp drift up because levels overlap in key space.
//!
//! The module exposes:
//! - [`TieredRun`] - the in-memory descriptor for one SSTable-equivalent run
//!   (id, byte size, sorted (key, optional value) pairs).
//! - [`TieredManifest`] - the list of runs per level.
//! - [`TieredCompactionPlanner`] - decides when to fire a merge and produces
//!   the merged run via [`Self::merge`].
//!
//! No file I/O here. The base [`crate::LsmTree`] owns the on-disk format;
//! this module is the pure planning + merging logic that a future compaction
//! thread (or test) drives.

use std::collections::BTreeMap;

/// One run of (key, optional value) entries. Tombstones are `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TieredRun {
    pub id: u64,
    pub size_bytes: u64,
    pub entries: Vec<(String, Option<Vec<u8>>)>,
}

impl TieredRun {
    pub fn new(id: u64, entries: Vec<(String, Option<Vec<u8>>)>) -> Self {
        let size_bytes = entries
            .iter()
            .map(|(k, v)| (k.len() + v.as_ref().map(|v| v.len()).unwrap_or(1)) as u64)
            .sum();
        Self {
            id,
            size_bytes,
            entries,
        }
    }
}

/// Per-level list of runs. `levels[0]` is the youngest tier (just flushed),
/// `levels[N]` is the oldest. Empty levels are simply empty `Vec`s.
#[derive(Debug, Default, Clone)]
pub struct TieredManifest {
    pub levels: Vec<Vec<TieredRun>>,
}

impl TieredManifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, level: usize, run: TieredRun) {
        while self.levels.len() <= level {
            self.levels.push(Vec::new());
        }
        self.levels[level].push(run);
    }

    pub fn level_run_count(&self, level: usize) -> usize {
        self.levels.get(level).map(|v| v.len()).unwrap_or(0)
    }

    pub fn total_run_count(&self) -> usize {
        self.levels.iter().map(|l| l.len()).sum()
    }
}

/// Plans tiered compactions: a level is "full" when it holds at least
/// `runs_per_level` runs. The planner is stateless; it just looks at the
/// manifest and returns the lowest level that needs a merge.
pub struct TieredCompactionPlanner {
    runs_per_level: usize,
}

impl TieredCompactionPlanner {
    /// `runs_per_level` is the threshold for firing a compaction (Cassandra's
    /// `min_threshold` analogue). Below 2 is degenerate; clamp to 2.
    pub fn new(runs_per_level: usize) -> Self {
        Self {
            runs_per_level: runs_per_level.max(2),
        }
    }

    pub fn runs_per_level(&self) -> usize {
        self.runs_per_level
    }

    /// Lowest level (by index) that has >= `runs_per_level` runs, or `None`.
    pub fn pick_level(&self, manifest: &TieredManifest) -> Option<usize> {
        manifest
            .levels
            .iter()
            .enumerate()
            .find(|(_, runs)| runs.len() >= self.runs_per_level)
            .map(|(i, _)| i)
    }

    /// Merge every run at `level` into a single new run, producing a manifest
    /// update where `level` is empty and `level + 1` gains the merged run.
    /// Newer-run-wins on key collisions (caller MUST push in order: newest last).
    pub fn merge(&self, manifest: &mut TieredManifest, level: usize, new_id: u64) {
        let runs = std::mem::take(&mut manifest.levels[level]);
        let merged_entries = merge_runs(&runs);
        let merged = TieredRun::new(new_id, merged_entries);
        manifest.push(level + 1, merged);
    }
}

/// Merge runs into a single sorted entry list. Later runs (higher index)
/// shadow earlier runs on identical keys. Tombstones are preserved at this
/// layer; the LSM read path decides when they can be dropped.
fn merge_runs(runs: &[TieredRun]) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    for run in runs {
        for (k, v) in &run.entries {
            out.insert(k.clone(), v.clone());
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
#[path = "tiered_compaction_tests.rs"]
mod tests;
