//! Point-in-time read snapshots over the SSTable manifest.
//!
//! A [`Snapshot`] captures the manifest (list of SSTable IDs) at the moment
//! it is taken. Readers holding a snapshot see the same set of runs even
//! while the writer flushes new memtables or compaction rewrites the
//! manifest underneath them.
//!
//! The snapshot itself is just an immutable `Arc<SnapshotManifest>` so
//! cloning is O(1) and zero-copy. The [`SnapshotManager`] owns the *current*
//! manifest as `Arc::clone`-friendly state; mutations swap a new Arc in
//! place via `Mutex<Arc<...>>`, leaving outstanding snapshots untouched.
//!
//! No on-disk garbage-collection of SSTable files. The base [`crate::LsmTree`]
//! file lifecycle is owned elsewhere; a real system would refcount file
//! deletion against live snapshots. This module's contract is just the
//! in-memory manifest isolation.

use std::sync::{Arc, Mutex};

/// Immutable per-snapshot view: an ordered list of SSTable ids that were
/// live when the snapshot was opened. Newest-last; the LSM read path walks
/// this in reverse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub sstable_ids: Vec<u64>,
}

impl SnapshotManifest {
    pub fn new(sstable_ids: Vec<u64>) -> Self {
        Self { sstable_ids }
    }
    pub fn len(&self) -> usize {
        self.sstable_ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.sstable_ids.is_empty()
    }
}

/// Handle to one read view. Cloning is cheap (`Arc` bump).
#[derive(Debug, Clone)]
pub struct Snapshot {
    manifest: Arc<SnapshotManifest>,
    /// Monotonic id so callers can distinguish snapshots in logs.
    id: u64,
}

impl Snapshot {
    pub fn manifest(&self) -> &SnapshotManifest {
        &self.manifest
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn sstable_ids(&self) -> &[u64] {
        &self.manifest.sstable_ids
    }
}

/// Owns the current manifest. Writers call [`Self::publish`] to swap in a
/// new manifest; readers call [`Self::snapshot`] to take a stable view.
pub struct SnapshotManager {
    current: Mutex<Arc<SnapshotManifest>>,
    next_snapshot_id: Mutex<u64>,
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self::with_initial(SnapshotManifest::new(Vec::new()))
    }

    pub fn with_initial(initial: SnapshotManifest) -> Self {
        Self {
            current: Mutex::new(Arc::new(initial)),
            next_snapshot_id: Mutex::new(0),
        }
    }

    /// Replace the current manifest. Existing snapshots keep their Arc and
    /// are unaffected.
    pub fn publish(&self, manifest: SnapshotManifest) {
        *self.current.lock().unwrap() = Arc::new(manifest);
    }

    /// Convenience helper: build a new manifest by transforming the current
    /// one. The closure runs under the lock and returns the new manifest.
    pub fn publish_with<F>(&self, f: F)
    where
        F: FnOnce(&SnapshotManifest) -> SnapshotManifest,
    {
        let mut guard = self.current.lock().unwrap();
        let next = f(&guard);
        *guard = Arc::new(next);
    }

    /// Take a stable, immutable view of the current manifest.
    pub fn snapshot(&self) -> Snapshot {
        let manifest = Arc::clone(&self.current.lock().unwrap());
        let mut id = self.next_snapshot_id.lock().unwrap();
        let snap_id = *id;
        *id += 1;
        Snapshot {
            manifest,
            id: snap_id,
        }
    }

    /// Visible to the *next* read, not held snapshots. Useful for sanity
    /// checks in tests and the LSM read path.
    pub fn current_ids(&self) -> Vec<u64> {
        self.current.lock().unwrap().sstable_ids.clone()
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
