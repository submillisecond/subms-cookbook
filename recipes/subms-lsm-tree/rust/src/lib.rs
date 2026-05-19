//! Minimal log-structured merge tree.
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

#[cfg(feature = "harness")]
pub mod recipe;

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
        })
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

    pub fn flush(&mut self) -> io::Result<()> {
        if self.memtable.is_empty() {
            return Ok(());
        }
        let path = self
            .data_dir
            .join(format!("sst-{:012}.dat", self.next_seq));
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
        }
        Ok(())
    }
}

impl Drop for LsmTree {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
