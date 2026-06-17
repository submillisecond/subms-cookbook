//! Per-instance counters for an ART.
//!
//! `MeasuredArt<V>` wraps an `Art<V>` and bumps a `Cell`-backed counter
//! on every operation. `metrics()` returns a `ArtMetrics` snapshot:
//! lookup / insert / delete counts, the depth of the LAST operation
//! (key length is depth in this no-path-compression implementation),
//! and a `NodeTypeCounts` distribution over Small / Full nodes.
//!
//! Counter overflow: each counter is a `u64`, which only saturates
//! after ~5.8 centuries at a billion ops/sec. Treated as effectively
//! unbounded; `saturating_add` is used so a degenerate workload that
//! ran for centuries would saturate rather than wrap.

use std::cell::Cell;

use crate::{Art, Children, Node};

pub struct MeasuredArt<V> {
    inner: Art<V>,
    lookups: Cell<u64>,
    insertions: Cell<u64>,
    deletions: Cell<u64>,
    last_depth: Cell<u32>,
}

impl<V> Default for MeasuredArt<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> MeasuredArt<V> {
    pub fn new() -> Self {
        Self {
            inner: Art::new(),
            lookups: Cell::new(0),
            insertions: Cell::new(0),
            deletions: Cell::new(0),
            last_depth: Cell::new(0),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        self.insertions.set(self.insertions.get().saturating_add(1));
        self.last_depth.set(key.len() as u32);
        self.inner.insert(key, value)
    }

    pub fn get(&self, key: &[u8]) -> Option<&V> {
        self.lookups.set(self.lookups.get().saturating_add(1));
        self.last_depth.set(key.len() as u32);
        self.inner.get(key)
    }

    pub fn delete(&mut self, key: &[u8]) -> Option<V> {
        self.deletions.set(self.deletions.get().saturating_add(1));
        self.last_depth.set(key.len() as u32);
        self.inner.delete_value(key)
    }

    /// Borrow the underlying ART read-only. Composes with `range-scan`
    /// or `serialize` features the consumer may also have enabled.
    pub fn tree(&self) -> &Art<V> {
        &self.inner
    }

    /// Borrow the underlying ART mutably. Bypasses the counters - if
    /// the consumer mutates through this they're outside the metrics
    /// view; that is the intent (e.g. running `compact()` on it).
    pub fn tree_mut(&mut self) -> &mut Art<V> {
        &mut self.inner
    }

    pub fn metrics(&self) -> ArtMetrics {
        let nodes = count_nodes(self.inner.root());
        ArtMetrics {
            lookups: self.lookups.get(),
            insertions: self.insertions.get(),
            deletions: self.deletions.get(),
            last_depth: self.last_depth.get(),
            node_types: nodes,
            entries: self.inner.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtMetrics {
    pub lookups: u64,
    pub insertions: u64,
    pub deletions: u64,
    pub last_depth: u32,
    pub node_types: NodeTypeCounts,
    pub entries: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeTypeCounts {
    pub small: u32,
    pub full: u32,
}

fn count_nodes<V>(node: &Node<V>) -> NodeTypeCounts {
    let mut acc = NodeTypeCounts::default();
    walk(node, &mut acc);
    acc
}

fn walk<V>(node: &Node<V>, acc: &mut NodeTypeCounts) {
    match &node.children {
        Children::Small {
            children, count, ..
        } => {
            acc.small += 1;
            for slot in children.iter().take(*count as usize) {
                if let Some(c) = slot.as_deref() {
                    walk(c, acc);
                }
            }
        }
        Children::Full(map) => {
            acc.full += 1;
            for child in map.values() {
                walk(child, acc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metrics_zero_everywhere() {
        let m: MeasuredArt<u32> = MeasuredArt::new();
        let snap = m.metrics();
        assert_eq!(snap.lookups, 0);
        assert_eq!(snap.insertions, 0);
        assert_eq!(snap.deletions, 0);
        assert_eq!(snap.last_depth, 0);
        assert_eq!(snap.entries, 0);
        // Root node always exists - it counts as Small at construction.
        assert_eq!(snap.node_types.small, 1);
        assert_eq!(snap.node_types.full, 0);
    }

    #[test]
    fn counters_track_operations() {
        let mut m: MeasuredArt<u32> = MeasuredArt::new();
        m.insert(b"alice", 1);
        m.insert(b"bob", 2);
        m.insert(b"alex", 3);
        let _ = m.get(b"alice");
        let _ = m.get(b"missing");
        let _ = m.delete(b"alex");
        let snap = m.metrics();
        assert_eq!(snap.insertions, 3);
        assert_eq!(snap.lookups, 2);
        assert_eq!(snap.deletions, 1);
        assert_eq!(snap.entries, 2);
    }

    #[test]
    fn last_depth_reflects_key_length() {
        let mut m: MeasuredArt<u32> = MeasuredArt::new();
        m.insert(b"abcdefghij", 1);
        assert_eq!(m.metrics().last_depth, 10);
        let _ = m.get(b"x");
        assert_eq!(m.metrics().last_depth, 1);
        let _ = m.delete(b"abcdefghij");
        assert_eq!(m.metrics().last_depth, 10);
    }

    #[test]
    fn node_type_distribution_changes_with_growth() {
        let mut m: MeasuredArt<u32> = MeasuredArt::new();
        // Up to 4 distinct first bytes -> root stays Small.
        for i in 0..4u8 {
            m.insert(&[i], i as u32);
        }
        let before = m.metrics().node_types;
        assert_eq!(before.full, 0, "root still Small: {before:?}");

        // 5th distinct first byte forces root to promote to Full.
        m.insert(&[4u8], 4);
        let after = m.metrics().node_types;
        assert_eq!(after.full, 1, "root promoted: {after:?}");
    }

    #[test]
    fn counter_overflow_uses_saturating_arithmetic() {
        // Force the counters to within striking distance of u64::MAX.
        let m: MeasuredArt<u32> = MeasuredArt::new();
        m.lookups.set(u64::MAX - 1);
        let _ = m.get(b"x");
        assert_eq!(m.metrics().lookups, u64::MAX);
        let _ = m.get(b"y");
        assert_eq!(m.metrics().lookups, u64::MAX, "saturating, not wrapping");
    }

    #[test]
    fn metrics_snapshot_is_independent_of_subsequent_ops() {
        let mut m: MeasuredArt<u32> = MeasuredArt::new();
        m.insert(b"a", 1);
        let snap = m.metrics();
        m.insert(b"b", 2);
        m.insert(b"c", 3);
        // The snapshot does not move.
        assert_eq!(snap.insertions, 1);
        assert_eq!(snap.entries, 1);
        // Re-read picks up the new state.
        assert_eq!(m.metrics().insertions, 3);
    }

    #[test]
    fn tree_accessor_composes_with_base_api() {
        let mut m: MeasuredArt<u32> = MeasuredArt::new();
        m.insert(b"alpha", 1);
        m.insert(b"beta", 2);
        // The wrapped tree is queriable through the inner reference.
        assert_eq!(m.tree().get(b"alpha").copied(), Some(1));
        assert_eq!(m.tree().get(b"beta").copied(), Some(2));
        // Inner lookups go through `tree()`, not through `get()`, so
        // the lookup counter stays at 0.
        assert_eq!(m.metrics().lookups, 0);
    }
}
