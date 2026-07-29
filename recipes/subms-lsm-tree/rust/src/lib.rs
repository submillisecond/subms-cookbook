//! Minimal log-structured merge tree.
//!
//! ```
//! use subms_lsm_tree::LsmTree;
//!
//! # fn main() -> std::io::Result<()> {
//! let dir = std::env::temp_dir().join("subms-lsm-doctest");
//! # std::fs::remove_dir_all(&dir).ok();
//! let mut lsm = LsmTree::open(&dir, 16_000)?;
//! lsm.put("AAPL", b"150.10")?;
//! assert_eq!(lsm.get("AAPL")?.as_deref(), Some(&b"150.10"[..])); // stored key: a hit
//! assert_eq!(lsm.get("ZZZZ")?, None);                            // absent: bloom-accelerated miss
//! # std::fs::remove_dir_all(&dir).ok();
//! # Ok(())
//! # }
//! ```
//!
//! Writes land in the memtable. When the memtable exceeds
//! `flush_threshold_bytes`, it is written as a new SSTable (with a bloom
//! filter trailer) and cleared. Reads check the memtable first, then
//! SSTables newest-to-oldest; with [`BloomMode::On`] each SSTable consults
//! its bloom filter before scanning, so misses short-circuit in a few hash
//! probes. With [`BloomMode::Off`] the bloom probe is skipped entirely -
//! useful for measuring how much the optimisation buys you. First hit wins,
//! tombstones included.
//!
//! Single-threaded by construction. No compaction, no WAL.

mod memtable;
mod sstable;

#[cfg(test)]
#[path = "lsm_tree_tests.rs"]
mod lsm_tree_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Each is gated by its own Cargo feature flag;
// `cargo add subms-lsm-tree` keeps the base build identical to 0.4.
//
// See README + cookbook page for per-feature p99, memory cost, and
// composition guidance.
#[cfg(any(
    feature = "wal",
    feature = "tiered-compaction",
    feature = "leveled-compaction",
    feature = "snapshot",
    feature = "lz4",
    feature = "zstd",
    feature = "block-cache-integration",
))]
pub mod features;

#[cfg(feature = "block-cache-integration")]
pub use features::block_cache_integration::{Block, BlockCache, BlockKey, LruBlockCache};
#[cfg(feature = "leveled-compaction")]
pub use features::leveled_compaction::{LeveledCompactionPlanner, LeveledManifest, LeveledRun};
#[cfg(feature = "lz4")]
pub use features::lz4::Lz4BlockCompressor;
#[cfg(feature = "snapshot")]
pub use features::snapshot::{Snapshot, SnapshotManager, SnapshotManifest};
#[cfg(feature = "tiered-compaction")]
pub use features::tiered_compaction::{TieredCompactionPlanner, TieredManifest, TieredRun};
#[cfg(feature = "wal")]
pub use features::wal::WriteAheadLog;
#[cfg(feature = "zstd")]
pub use features::zstd::ZstdBlockCompressor;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use memtable::Memtable;
use sstable::SsTable;

/// Read-path bloom-filter behaviour. The filter is always *written* into
/// every SSTable trailer - this just controls whether reads consult it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BloomMode {
    /// Check the bloom filter before scanning each SSTable. Default.
    On,
    /// Skip the bloom probe. Every SSTable in the walk pays a full scan.
    Off,
}

pub struct LsmTree {
    data_dir: PathBuf,
    flush_threshold_bytes: usize,
    memtable: Memtable,
    sstables: Vec<SsTable>,
    next_seq: u64,
    bloom_mode: BloomMode,
    /// Auto-compaction trigger: when the on-disk run count reaches this, a flush
    /// merges every run into one, reclaiming superseded versions. 0 = disabled
    /// (the base tree's documented no-automatic-compaction behaviour). Opt in via
    /// [`Self::set_compaction_trigger`].
    compaction_trigger: usize,
}

impl LsmTree {
    /// Equivalent to [`Self::open_with`] with [`BloomMode::On`].
    pub fn open(data_dir: impl AsRef<Path>, flush_threshold_bytes: usize) -> io::Result<Self> {
        Self::open_with(data_dir, flush_threshold_bytes, BloomMode::On)
    }

    pub fn open_with(
        data_dir: impl AsRef<Path>,
        flush_threshold_bytes: usize,
        bloom_mode: BloomMode,
    ) -> io::Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&data_dir)?;

        let mut files: Vec<PathBuf> = fs::read_dir(&data_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("sst-"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();

        let next_seq = files
            .last()
            .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
            .and_then(|stem| stem.strip_prefix("sst-"))
            .and_then(|n| n.parse::<u64>().ok())
            .map(|n| n + 1)
            .unwrap_or(0);

        let mut sstables = Vec::with_capacity(files.len());
        for f in files {
            sstables.push(SsTable::open(f)?);
        }

        Ok(Self {
            data_dir,
            flush_threshold_bytes,
            memtable: Memtable::new(),
            sstables,
            next_seq,
            bloom_mode,
            compaction_trigger: 0,
        })
    }

    /// Enable automatic compaction: once the tree accumulates `trigger` on-disk
    /// runs, the next flush merges them all into one, dropping every superseded
    /// version and tombstone. `trigger = 0` disables it (the default). This is
    /// what bounds on-disk size under overwrite-heavy workloads - without it,
    /// every flush leaves a fresh run and the dead versions in older runs are
    /// never reclaimed. Returns `self` for builder-style construction.
    pub fn set_compaction_trigger(&mut self, trigger: usize) -> &mut Self {
        self.compaction_trigger = trigger;
        self
    }

    /// The current auto-compaction trigger (0 = disabled).
    pub fn compaction_trigger(&self) -> usize {
        self.compaction_trigger
    }

    /// Merge every on-disk run into a single run, keeping only the newest value
    /// per key and discarding superseded versions and tombstones. Safe to call
    /// manually at any time; a no-op when there are fewer than two runs.
    pub fn compact(&mut self) -> io::Result<()> {
        if self.sstables.len() < 2 {
            return Ok(());
        }
        // Runs are ordered oldest -> newest, so a later run's value for a key
        // wins. A full merge has no older run left to shadow, so a tombstone just
        // drops the key entirely.
        let mut merged: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        for sst in &self.sstables {
            for (key, value) in sst.entries() {
                merged.insert(key, value);
            }
        }
        let live: Vec<(String, Vec<u8>)> = merged
            .into_iter()
            .filter_map(|(k, v)| v.map(|val| (k, val)))
            .collect();

        let path = self.data_dir.join(format!("sst-{:012}.dat", self.next_seq));
        self.next_seq += 1;
        let new_sst = SsTable::write(
            &path,
            live.len(),
            live.iter().map(|(k, v)| (k.as_str(), Some(v.as_slice()))),
        )?;

        let old_paths: Vec<PathBuf> = self
            .sstables
            .iter()
            .map(|s| s.path().to_path_buf())
            .collect();
        self.sstables.clear();
        self.sstables.push(new_sst);
        for p in old_paths {
            let _ = fs::remove_file(p);
        }
        Ok(())
    }

    pub fn put(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        self.memtable.put(key, Some(value.to_vec()));
        self.maybe_flush()
    }

    pub fn delete(&mut self, key: &str) -> io::Result<()> {
        self.memtable.put(key, None);
        self.maybe_flush()
    }

    /// Returns `None` for absent *or* tombstoned keys.
    pub fn get(&self, key: &str) -> io::Result<Option<Vec<u8>>> {
        if let Some(hit) = self.memtable.get(key) {
            return Ok(hit.map(|v| v.to_vec()));
        }
        let check_bloom = matches!(self.bloom_mode, BloomMode::On);
        for sst in self.sstables.iter().rev() {
            if let Some(hit) = sst.get(key, check_bloom) {
                return Ok(hit);
            }
        }
        Ok(None)
    }

    /// Every live key in `[lo, hi)` (either bound `None` = unbounded), in sorted
    /// key order, as owned `(key, value)` pairs. Merges the memtable over every
    /// on-disk run newest-first: the newest write per key wins and tombstoned
    /// keys are omitted - the same resolution as [`Self::get`], across a range.
    pub fn range(&self, lo: Option<&str>, hi: Option<&str>) -> io::Result<Vec<(String, Vec<u8>)>> {
        // Newest source first: memtable, then runs newest -> oldest. `or_insert`
        // keeps the first (newest) value seen for a key; `None` marks a tombstone,
        // dropped in the final pass so a delete shadows older runs.
        let mut merged: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        for (k, v) in self.memtable.range(lo, hi) {
            merged
                .entry(k.to_string())
                .or_insert_with(|| v.map(|s| s.to_vec()));
        }
        for sst in self.sstables.iter().rev() {
            for (k, v) in sst.range(lo, hi) {
                merged.entry(k).or_insert(v);
            }
        }
        Ok(merged
            .into_iter()
            .filter_map(|(k, v)| v.map(|val| (k, val)))
            .collect())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if self.memtable.is_empty() {
            return Ok(());
        }
        let path = self.data_dir.join(format!("sst-{:012}.dat", self.next_seq));
        self.next_seq += 1;
        let sst = SsTable::write(
            path,
            self.memtable.entry_count(),
            self.memtable.sorted_entries(),
        )?;
        self.sstables.push(sst);
        self.memtable.clear();
        Ok(())
    }

    pub fn sstable_count(&self) -> usize {
        self.sstables.len()
    }

    pub fn bloom_mode(&self) -> BloomMode {
        self.bloom_mode
    }

    fn maybe_flush(&mut self) -> io::Result<()> {
        if self.memtable.approx_size_bytes() >= self.flush_threshold_bytes {
            self.flush()?;
            // Opt-in auto-compaction: bound the run count (and reclaim dead
            // versions) once it reaches the trigger.
            if self.compaction_trigger > 0 && self.sstables.len() >= self.compaction_trigger {
                self.compact()?;
            }
        }
        Ok(())
    }
}

impl Drop for LsmTree {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
