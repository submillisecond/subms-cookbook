//! Leveled compaction (RocksDB style): level L has a soft size budget of
//! `base * fanout^L` bytes. Each level L > 0 holds runs with disjoint key
//! ranges. Compacting from level L picks one run, finds all overlapping
//! runs at L+1, and merges them into a set of non-overlapping output runs.
//!
//! Tradeoff vs tiered: leveled holds read amplification down (one run per
//! key at any level beyond L0) at the cost of higher write amplification
//! (a record can be rewritten ~fanout times moving from L to L+1).
//!
//! The module exposes:
//! - [`LeveledRun`] - a sorted entry list plus its first/last key fences.
//! - [`LeveledManifest`] - the per-level run list.
//! - [`LeveledCompactionPlanner`] - decides which level + run to compact
//!   and produces the merge via [`Self::compact`].
//!
//! No file I/O. Pure manifest + merge planning.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeveledRun {
    pub id: u64,
    pub entries: Vec<(String, Option<Vec<u8>>)>,
}

impl LeveledRun {
    pub fn new(id: u64, mut entries: Vec<(String, Option<Vec<u8>>)>) -> Self {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Self { id, entries }
    }

    pub fn size_bytes(&self) -> u64 {
        self.entries
            .iter()
            .map(|(k, v)| (k.len() + v.as_ref().map(|v| v.len()).unwrap_or(1)) as u64)
            .sum()
    }

    pub fn min_key(&self) -> Option<&str> {
        self.entries.first().map(|(k, _)| k.as_str())
    }

    pub fn max_key(&self) -> Option<&str> {
        self.entries.last().map(|(k, _)| k.as_str())
    }

    fn overlaps(&self, other: &LeveledRun) -> bool {
        match (
            self.min_key(),
            self.max_key(),
            other.min_key(),
            other.max_key(),
        ) {
            (Some(amin), Some(amax), Some(bmin), Some(bmax)) => !(amax < bmin || bmax < amin),
            _ => false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct LeveledManifest {
    pub levels: Vec<Vec<LeveledRun>>,
}

impl LeveledManifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, level: usize, run: LeveledRun) {
        while self.levels.len() <= level {
            self.levels.push(Vec::new());
        }
        self.levels[level].push(run);
        if level > 0 {
            // L > 0 keeps runs sorted by min_key so overlap selection is cheap.
            self.levels[level].sort_by(|a, b| a.min_key().cmp(&b.min_key()));
        }
    }

    pub fn level_run_count(&self, level: usize) -> usize {
        self.levels.get(level).map(|v| v.len()).unwrap_or(0)
    }

    pub fn level_bytes(&self, level: usize) -> u64 {
        self.levels
            .get(level)
            .map(|runs| runs.iter().map(|r| r.size_bytes()).sum())
            .unwrap_or(0)
    }
}

pub struct LeveledCompactionPlanner {
    /// Base size for level 1 in bytes; level L's budget is `base * fanout^(L - 1)`.
    base_bytes: u64,
    /// Per-level multiplier; default 10 matches RocksDB.
    fanout: u64,
    /// Max number of L0 runs before L0 must be flushed.
    l0_run_limit: usize,
}

impl LeveledCompactionPlanner {
    pub fn new(base_bytes: u64, fanout: u64, l0_run_limit: usize) -> Self {
        Self {
            base_bytes: base_bytes.max(1),
            fanout: fanout.max(2),
            l0_run_limit: l0_run_limit.max(1),
        }
    }

    pub fn base_bytes(&self) -> u64 {
        self.base_bytes
    }
    pub fn fanout(&self) -> u64 {
        self.fanout
    }
    pub fn l0_run_limit(&self) -> usize {
        self.l0_run_limit
    }

    /// Budget for level L (L >= 1). L0 doesn't carry a byte budget - it's
    /// run-count limited.
    pub fn level_budget(&self, level: usize) -> u64 {
        if level == 0 {
            return 0;
        }
        let mut budget = self.base_bytes;
        for _ in 1..level {
            budget = budget.saturating_mul(self.fanout);
        }
        budget
    }

    /// Lowest level that exceeds its budget (or L0 above its run limit).
    /// Returns the level to compact FROM.
    pub fn pick_level(&self, manifest: &LeveledManifest) -> Option<usize> {
        if manifest.level_run_count(0) >= self.l0_run_limit {
            return Some(0);
        }
        for (l, runs) in manifest.levels.iter().enumerate().skip(1) {
            if runs.is_empty() {
                continue;
            }
            let budget = self.level_budget(l);
            let bytes: u64 = runs.iter().map(|r| r.size_bytes()).sum();
            if bytes > budget {
                return Some(l);
            }
        }
        None
    }

    /// Compact from `from_level` into `from_level + 1`.
    ///
    /// L0 -> L1 path: every L0 run is taken (they can overlap among each
    /// other), then all overlapping L1 runs are picked up; output replaces
    /// the consumed runs in L1.
    ///
    /// L>=1 -> next: one run is selected (the first by min_key for
    /// determinism), all overlapping runs at L+1 are picked up, output
    /// replaces them.
    ///
    /// `next_id` is the id assigned to the merged output (a planner counter
    /// owned by the caller).
    pub fn compact(&self, manifest: &mut LeveledManifest, from_level: usize, next_id: u64) {
        let inputs_from = if from_level == 0 {
            std::mem::take(&mut manifest.levels[0])
        } else {
            // Pick the leftmost run as the seed.
            let l = &mut manifest.levels[from_level];
            if l.is_empty() {
                return;
            }
            vec![l.remove(0)]
        };

        // Determine key range covered by the picked runs.
        let mut min_key: Option<String> = None;
        let mut max_key: Option<String> = None;
        for r in &inputs_from {
            if let Some(k) = r.min_key() {
                min_key = Some(min_key.map_or(k.to_string(), |cur| cur.min(k.to_string())));
            }
            if let Some(k) = r.max_key() {
                max_key = Some(max_key.map_or(k.to_string(), |cur| cur.max(k.to_string())));
            }
        }
        let dest_level = from_level + 1;
        while manifest.levels.len() <= dest_level {
            manifest.levels.push(Vec::new());
        }

        // Pluck overlapping destination runs.
        let mut overlapping_dst: Vec<LeveledRun> = Vec::new();
        if let (Some(min), Some(max)) = (&min_key, &max_key) {
            let dst = &mut manifest.levels[dest_level];
            let mut keep = Vec::with_capacity(dst.len());
            for r in dst.drain(..) {
                let rmin = r.min_key().unwrap_or("");
                let rmax = r.max_key().unwrap_or("");
                let overlaps = !(rmax < min.as_str() || rmin > max.as_str());
                if overlaps {
                    overlapping_dst.push(r);
                } else {
                    keep.push(r);
                }
            }
            *dst = keep;
        }

        // Merge: latest writes win on key collisions. For L0->L1, the inputs
        // come in oldest-first order from the manifest (we pushed in that
        // order); for L>=1 there's at most one input. Then destination
        // overlapping runs are NEWER than the input only for L=0 case? No -
        // destination is always OLDER. So: drop overlapping_dst into the map
        // first, then write inputs_from on top so they shadow.
        let mut out: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        for r in overlapping_dst {
            for (k, v) in r.entries {
                out.insert(k, v);
            }
        }
        for r in inputs_from {
            for (k, v) in r.entries {
                out.insert(k, v);
            }
        }
        let merged: Vec<(String, Option<Vec<u8>>)> = out.into_iter().collect();
        if !merged.is_empty() {
            let new_run = LeveledRun::new(next_id, merged);
            manifest.levels[dest_level].push(new_run);
            manifest.levels[dest_level].sort_by(|a, b| a.min_key().cmp(&b.min_key()));
        }
    }
}

/// Returns `true` if every pair of runs at `level` is key-disjoint. Used by
/// tests + invariant checks. L0 is exempt (allowed to overlap).
pub fn level_is_non_overlapping(manifest: &LeveledManifest, level: usize) -> bool {
    let runs = match manifest.levels.get(level) {
        Some(r) => r,
        None => return true,
    };
    for i in 0..runs.len() {
        for j in (i + 1)..runs.len() {
            if runs[i].overlaps(&runs[j]) {
                return false;
            }
        }
    }
    true
}

impl LeveledManifest {
    pub fn total_run_count(&self) -> usize {
        self.levels.iter().map(|l| l.len()).sum()
    }
}

#[cfg(test)]
#[path = "leveled_compaction_tests.rs"]
mod tests;
