//! Log-structured merge tree with background flush.
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
//! Writes land in an in-memory `active` memtable. When it exceeds
//! `flush_threshold_bytes` the tree *rotates*: the full memtable is frozen and
//! a fresh one is installed so the triggering write returns immediately. By
//! default ([`FlushMode::Background`]) a background thread turns each frozen
//! memtable into an SSTable (with a bloom-filter trailer) off the write path,
//! so `put` never pays the O(memtable) flush cost - only the swap. Reads check
//! `active`, then the frozen memtables still awaiting flush (newest first),
//! then SSTables newest-to-oldest; with [`BloomMode::On`] each SSTable consults
//! its bloom filter before scanning, so misses short-circuit in a few hash
//! probes. First hit wins, tombstones included.
//!
//! [`FlushMode::Sync`] flushes inline on the calling thread instead - thread-free
//! and deterministic (for single-threaded / wasm targets or deterministic
//! replay), at the cost of a periodic write-latency spike on the write that
//! triggers a flush.
//!
//! Durability: a frozen memtable queued for flush is not on disk until the
//! worker writes it, so a hard crash loses the queued + active memtables unless
//! the `wal` feature is recording them for replay - the same no-durability-
//! without-WAL profile as before, just with a slightly wider in-memory window.
//! [`LsmTree::flush`] forces everything pending to disk and blocks until it is.

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

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

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

/// When and where a full memtable is turned into an SSTable.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FlushMode {
    /// The default. A full memtable is handed to a background thread and a fresh
    /// memtable is installed immediately, so the triggering write pays only the
    /// swap, not the O(memtable) flush. This is what keeps the write tail flat.
    Background,
    /// The triggering write flushes inline on the caller's thread. Deterministic
    /// and thread-free (single-threaded / wasm targets, deterministic replay) at
    /// the cost of a periodic write-latency spike.
    Sync,
}

/// Frozen memtables allowed to queue ahead of the background writer before a
/// `put` blocks (back-pressure). Bounds the in-memory overhang if writes ever
/// outrun the flush thread; at the recipe's workload the queue never fills.
const DEFAULT_MAX_IMMUTABLE: usize = 4;

/// State shared between the writer and the background flush worker.
struct Shared {
    state: Mutex<State>,
    signal: Condvar,
    data_dir: PathBuf,
    bloom_mode: BloomMode,
    max_immutable: usize,
}

struct State {
    /// Frozen memtables awaiting flush, oldest at the front. Reads still see
    /// them; the worker pops the front once its SSTable is registered.
    immutable: VecDeque<Arc<Memtable>>,
    /// On-disk runs, oldest -> newest, as one immutable snapshot. A reader clones
    /// this single `Arc` (O(1)) rather than the whole run list; registering a run
    /// rebuilds the vector (copy-on-write) - the rare path pays, the hot read does
    /// not.
    sstables: Arc<Vec<Arc<SsTable>>>,
    next_seq: u64,
    shutdown: bool,
    /// First error the background worker hit, surfaced to the next writer call.
    flush_err: Option<io::Error>,
}

pub struct LsmTree {
    /// Writer-local buffer of pending writes. The hot `put` path never locks.
    active: Memtable,
    shared: Arc<Shared>,
    /// Present once the background worker has been spawned (lazy, on first
    /// [`FlushMode::Background`] flush). `None` in [`FlushMode::Sync`].
    flush_handle: Option<JoinHandle<()>>,
    flush_threshold_bytes: usize,
    flush_mode: FlushMode,
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
            sstables.push(Arc::new(SsTable::open(f)?));
        }

        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                immutable: VecDeque::new(),
                sstables: Arc::new(sstables),
                next_seq,
                shutdown: false,
                flush_err: None,
            }),
            signal: Condvar::new(),
            data_dir,
            bloom_mode,
            max_immutable: DEFAULT_MAX_IMMUTABLE,
        });

        Ok(Self {
            active: Memtable::new(),
            shared,
            flush_handle: None,
            flush_threshold_bytes,
            flush_mode: FlushMode::Background,
            compaction_trigger: 0,
        })
    }

    /// Choose inline vs background flush. [`FlushMode::Background`] is the
    /// default; call this before the first write to opt into [`FlushMode::Sync`].
    /// Returns `self` for builder-style construction.
    pub fn set_flush_mode(&mut self, mode: FlushMode) -> &mut Self {
        self.flush_mode = mode;
        self
    }

    /// The active flush mode.
    pub fn flush_mode(&self) -> FlushMode {
        self.flush_mode
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
    /// manually at any time; a no-op when there are fewer than two runs. Drains
    /// any pending background flush first so the merge sees every run.
    pub fn compact(&mut self) -> io::Result<()> {
        self.enqueue_active()?;
        self.drain()?;

        let ssts = {
            let st = self.lock();
            st.sstables.clone()
        };
        if ssts.len() < 2 {
            return Ok(());
        }
        // Runs are ordered oldest -> newest, so a later run's value for a key
        // wins. A full merge has no older run left to shadow, so a tombstone just
        // drops the key entirely.
        let mut merged: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        for sst in ssts.iter() {
            for (key, value) in sst.entries() {
                merged.insert(key, value);
            }
        }
        let live: Vec<(String, Vec<u8>)> = merged
            .into_iter()
            .filter_map(|(k, v)| v.map(|val| (k, val)))
            .collect();

        let seq = self.reserve_seq();
        let path = self.shared.data_dir.join(format!("sst-{seq:012}.dat"));
        let new_sst = SsTable::write(
            &path,
            live.len(),
            live.iter().map(|(k, v)| (k.as_str(), Some(v.as_slice()))),
        )?;

        let old_paths: Vec<PathBuf> = ssts.iter().map(|s| s.path().to_path_buf()).collect();
        {
            let mut st = self.lock();
            // No flush can have raced us: we drained, and this tree is the only
            // writer, so `immutable` stayed empty and no new run was appended.
            st.sstables = Arc::new(vec![Arc::new(new_sst)]);
        }
        for p in old_paths {
            let _ = fs::remove_file(p);
        }
        Ok(())
    }

    pub fn put(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        self.active.put(key, Some(value.to_vec()));
        self.maybe_rotate()
    }

    pub fn delete(&mut self, key: &str) -> io::Result<()> {
        self.active.put(key, None);
        self.maybe_rotate()
    }

    /// Returns `None` for absent *or* tombstoned keys.
    pub fn get(&self, key: &str) -> io::Result<Option<Vec<u8>>> {
        if let Some(hit) = self.active.get(key) {
            return Ok(hit.map(|v| v.to_vec()));
        }
        let (imms, ssts) = self.snapshot();
        for m in &imms {
            if let Some(hit) = m.get(key) {
                return Ok(hit.map(|v| v.to_vec()));
            }
        }
        let check_bloom = matches!(self.shared.bloom_mode, BloomMode::On);
        for sst in ssts.iter().rev() {
            if let Some(hit) = sst.get(key, check_bloom) {
                return Ok(hit);
            }
        }
        Ok(None)
    }

    /// Every live key in `[lo, hi)` (either bound `None` = unbounded), in sorted
    /// key order, as owned `(key, value)` pairs. Merges the active memtable over
    /// the frozen memtables and every on-disk run newest-first: the newest write
    /// per key wins and tombstoned keys are omitted - the same resolution as
    /// [`Self::get`], across a range.
    pub fn range(&self, lo: Option<&str>, hi: Option<&str>) -> io::Result<Vec<(String, Vec<u8>)>> {
        // Newest source first: active memtable, frozen memtables newest -> oldest,
        // then runs newest -> oldest. `or_insert` keeps the first (newest) value
        // seen for a key; `None` marks a tombstone, dropped in the final pass so a
        // delete shadows older sources.
        let mut merged: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        for (k, v) in self.active.range(lo, hi) {
            merged
                .entry(k.to_string())
                .or_insert_with(|| v.map(|s| s.to_vec()));
        }
        let (imms, ssts) = self.snapshot();
        for m in &imms {
            for (k, v) in m.range(lo, hi) {
                merged
                    .entry(k.to_string())
                    .or_insert_with(|| v.map(|s| s.to_vec()));
            }
        }
        for sst in ssts.iter().rev() {
            for (k, v) in sst.range(lo, hi) {
                merged.entry(k).or_insert(v);
            }
        }
        Ok(merged
            .into_iter()
            .filter_map(|(k, v)| v.map(|val| (k, val)))
            .collect())
    }

    /// Force everything pending - the active memtable and any frozen memtables
    /// still queued - to disk, blocking until it is registered as SSTables. This
    /// is the deterministic sync point tests and callers rely on.
    pub fn flush(&mut self) -> io::Result<()> {
        self.enqueue_active()?;
        self.drain()
    }

    pub fn sstable_count(&self) -> usize {
        self.lock().sstables.len()
    }

    pub fn bloom_mode(&self) -> BloomMode {
        self.shared.bloom_mode
    }

    // ---- internals ----

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.shared.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A cheap, lock-free-after-clone view of everything a read must consult
    /// behind the active memtable: frozen memtables newest-first, plus the run
    /// list. Holding the lock only for the `Arc` clones keeps the flush worker
    /// and readers from serialising on the actual scan.
    fn snapshot(&self) -> (Vec<Arc<Memtable>>, Arc<Vec<Arc<SsTable>>>) {
        let st = self.lock();
        (
            st.immutable.iter().rev().cloned().collect(),
            Arc::clone(&st.sstables),
        )
    }

    fn reserve_seq(&self) -> u64 {
        let mut st = self.lock();
        let seq = st.next_seq;
        st.next_seq += 1;
        seq
    }

    fn maybe_rotate(&mut self) -> io::Result<()> {
        if self.active.approx_size_bytes() < self.flush_threshold_bytes {
            return Ok(());
        }
        self.enqueue_active()?;
        // Opt-in auto-compaction: bound the run count (and reclaim dead versions)
        // once it reaches the trigger. `compact` drains first, so a background
        // flush still in flight is accounted for.
        if self.compaction_trigger > 0 && self.sstable_count() >= self.compaction_trigger {
            self.compact()?;
        }
        Ok(())
    }

    /// Move the active memtable into the flush pipeline and install a fresh one.
    /// Background mode enqueues it for the worker (spawning it on first use);
    /// Sync mode writes the SSTable inline on this thread.
    fn enqueue_active(&mut self) -> io::Result<()> {
        if self.active.is_empty() {
            // Still surface a prior background error even when there is nothing
            // new to flush.
            return self.take_flush_err();
        }
        match self.flush_mode {
            FlushMode::Sync => self.flush_inline(),
            FlushMode::Background => {
                self.ensure_worker();
                let frozen = Arc::new(std::mem::replace(&mut self.active, Memtable::new()));
                let mut st = self.lock();
                if let Some(e) = st.flush_err.take() {
                    return Err(e);
                }
                while st.immutable.len() >= self.shared.max_immutable && !st.shutdown {
                    st = self
                        .shared
                        .signal
                        .wait(st)
                        .unwrap_or_else(|e| e.into_inner());
                }
                st.immutable.push_back(frozen);
                self.shared.signal.notify_all();
                Ok(())
            }
        }
    }

    /// Synchronous flush of the active memtable on the calling thread.
    fn flush_inline(&mut self) -> io::Result<()> {
        if self.active.is_empty() {
            return Ok(());
        }
        let seq = self.reserve_seq();
        let path = self.shared.data_dir.join(format!("sst-{seq:012}.dat"));
        let sst = SsTable::write(
            &path,
            self.active.entry_count(),
            self.active.sorted_entries(),
        )?;
        self.active.clear();
        push_sstable(&mut self.lock(), sst);
        Ok(())
    }

    /// Block until the frozen-memtable queue is empty (background mode only).
    fn drain(&mut self) -> io::Result<()> {
        if self.flush_handle.is_none() {
            return Ok(());
        }
        let mut st = self.lock();
        while !st.immutable.is_empty() && st.flush_err.is_none() {
            st = self
                .shared
                .signal
                .wait(st)
                .unwrap_or_else(|e| e.into_inner());
        }
        match st.flush_err.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn take_flush_err(&self) -> io::Result<()> {
        match self.lock().flush_err.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn ensure_worker(&mut self) {
        if self.flush_handle.is_some() {
            return;
        }
        let shared = Arc::clone(&self.shared);
        self.flush_handle = Some(std::thread::spawn(move || flush_worker(&shared)));
    }
}

/// Background flush loop: turn each frozen memtable into an SSTable off the
/// write path. The memtable stays in `immutable` (visible to readers) until its
/// SSTable is registered, so a key is never transiently invisible.
fn flush_worker(shared: &Shared) {
    loop {
        let frozen = {
            let mut st = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if !st.immutable.is_empty() {
                    break;
                }
                if st.shutdown {
                    return;
                }
                st = shared.signal.wait(st).unwrap_or_else(|e| e.into_inner());
            }
            Arc::clone(st.immutable.front().unwrap())
        };

        let seq = {
            let mut st = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let seq = st.next_seq;
            st.next_seq += 1;
            seq
        };
        let path = shared.data_dir.join(format!("sst-{seq:012}.dat"));
        let result = SsTable::write(&path, frozen.entry_count(), frozen.sorted_entries());

        let mut st = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(sst) => push_sstable(&mut st, sst),
            Err(e) => {
                if st.flush_err.is_none() {
                    st.flush_err = Some(e);
                }
            }
        }
        st.immutable.pop_front();
        shared.signal.notify_all();
    }
}

/// Append a run to the shared list, copy-on-write: readers holding the previous
/// `Arc<Vec<..>>` snapshot keep scanning it untouched while the new snapshot
/// takes its place.
fn push_sstable(st: &mut State, sst: SsTable) {
    let mut runs = (*st.sstables).clone();
    runs.push(Arc::new(sst));
    st.sstables = Arc::new(runs);
}

impl Drop for LsmTree {
    fn drop(&mut self) {
        if self.flush_handle.is_some() {
            // Persist the active memtable via the worker, then stop and join it so
            // every queued SSTable is on disk before the tree goes away.
            let _ = self.enqueue_active();
            {
                let mut st = self.lock();
                st.shutdown = true;
                self.shared.signal.notify_all();
            }
            if let Some(h) = self.flush_handle.take() {
                let _ = h.join();
            }
        } else {
            // Sync mode, or background that never spawned a worker: flush inline.
            let _ = self.flush_inline();
        }
    }
}
