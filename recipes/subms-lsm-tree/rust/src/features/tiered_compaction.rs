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
mod tests {
    use super::*;

    fn run(id: u64, kvs: &[(&str, Option<&[u8]>)]) -> TieredRun {
        TieredRun::new(
            id,
            kvs.iter()
                .map(|(k, v)| (k.to_string(), v.map(|b| b.to_vec())))
                .collect(),
        )
    }

    #[test]
    fn pick_level_finds_full_level() {
        let mut m = TieredManifest::new();
        m.push(0, run(1, &[("a", Some(b"1"))]));
        m.push(0, run(2, &[("b", Some(b"2"))]));
        m.push(0, run(3, &[("c", Some(b"3"))]));
        let planner = TieredCompactionPlanner::new(3);
        assert_eq!(planner.pick_level(&m), Some(0));
    }

    #[test]
    fn pick_level_returns_none_when_no_level_full() {
        let mut m = TieredManifest::new();
        m.push(0, run(1, &[("a", Some(b"1"))]));
        m.push(1, run(2, &[("b", Some(b"2"))]));
        let planner = TieredCompactionPlanner::new(3);
        assert!(planner.pick_level(&m).is_none());
    }

    #[test]
    fn merge_promotes_to_next_level() {
        let mut m = TieredManifest::new();
        m.push(0, run(1, &[("a", Some(b"1"))]));
        m.push(0, run(2, &[("b", Some(b"2"))]));
        m.push(0, run(3, &[("c", Some(b"3"))]));
        let planner = TieredCompactionPlanner::new(3);
        planner.merge(&mut m, 0, 100);
        assert_eq!(m.level_run_count(0), 0, "level 0 emptied");
        assert_eq!(m.level_run_count(1), 1, "level 1 gained the merged run");
        let merged = &m.levels[1][0];
        assert_eq!(merged.id, 100);
        let keys: Vec<&str> = merged.entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn newer_run_wins_on_key_collision() {
        let mut m = TieredManifest::new();
        m.push(0, run(1, &[("k", Some(b"old"))]));
        m.push(0, run(2, &[("k", Some(b"new"))]));
        let planner = TieredCompactionPlanner::new(2);
        planner.merge(&mut m, 0, 50);
        let merged = &m.levels[1][0];
        assert_eq!(merged.entries.len(), 1);
        assert_eq!(merged.entries[0].1.as_deref(), Some(&b"new"[..]));
    }

    #[test]
    fn tombstone_is_preserved_in_merge() {
        let mut m = TieredManifest::new();
        m.push(0, run(1, &[("k", Some(b"v"))]));
        m.push(0, run(2, &[("k", None)]));
        let planner = TieredCompactionPlanner::new(2);
        planner.merge(&mut m, 0, 50);
        let merged = &m.levels[1][0];
        assert_eq!(merged.entries.len(), 1);
        assert!(merged.entries[0].1.is_none(), "tombstone wins");
    }

    #[test]
    fn runs_per_level_floor_is_two() {
        let planner = TieredCompactionPlanner::new(0);
        assert_eq!(planner.runs_per_level(), 2);
        let planner = TieredCompactionPlanner::new(1);
        assert_eq!(planner.runs_per_level(), 2);
    }

    #[test]
    fn merge_handles_non_overlapping_keys() {
        let mut m = TieredManifest::new();
        m.push(0, run(1, &[("a", Some(b"1")), ("c", Some(b"3"))]));
        m.push(0, run(2, &[("b", Some(b"2")), ("d", Some(b"4"))]));
        let planner = TieredCompactionPlanner::new(2);
        planner.merge(&mut m, 0, 99);
        let merged = &m.levels[1][0];
        let keys: Vec<&str> = merged.entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn cascading_compaction_via_repeated_pick_and_merge() {
        let mut m = TieredManifest::new();
        for i in 0..4 {
            m.push(0, run(i, &[(&format!("k{i}"), Some(b"v"))]));
        }
        let planner = TieredCompactionPlanner::new(4);
        let lvl = planner.pick_level(&m).unwrap();
        planner.merge(&mut m, lvl, 10);
        assert_eq!(
            planner.pick_level(&m),
            None,
            "single merged run does not trigger again"
        );
        assert_eq!(m.level_run_count(1), 1);
        assert_eq!(m.levels[1][0].entries.len(), 4);
    }
}
