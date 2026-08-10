//! Read-only snapshot wrapper for concurrent readers.
//!
//! `TreapSnapshot<K, V>` is an `Arc<Inner>` over an immutable, sorted
//! `(K, V)` vector built from the source treap at snapshot time.
//! Readers `clone()` the `Arc` (cheap pointer bump) and share the
//! same frozen data across threads. The source treap can continue
//! to take inserts / deletes without disturbing held snapshots.
//!
//! Why a sorted vec, not a frozen tree? The bench numbers favour
//! cache-dense binary search over pointer-chase traversal for the
//! read-only path. We also get `O(N)` ordered iteration trivially.
//! The cost is `O(N log N)` snapshot construction up-front - the
//! right trade-off when snapshots are taken seldom and read often.
//!
//! Composition: combine with `range-query` for sorted-range
//! iteration over the snapshot (use `TreapSnapshot::range`), or with
//! the persistent treap for a no-allocation version-pin.

use std::sync::Arc;

use crate::{NIL, Treap};

struct Inner<K, V> {
    /// Sorted by key. Binary-searchable; iteration is cache-dense.
    data: Vec<(K, V)>,
}

pub struct TreapSnapshot<K, V> {
    inner: Arc<Inner<K, V>>,
}

impl<K: Ord + Clone, V: Clone> TreapSnapshot<K, V> {
    /// Build a frozen snapshot from the current state of `treap`.
    /// `O(N)` time + `O(N)` allocation; the result is shareable
    /// across threads via `Clone` of the snapshot (refcount bump).
    pub fn from_treap(treap: &Treap<K, V>) -> Self {
        let mut data = Vec::with_capacity(treap.len());
        collect(treap, treap.root, &mut data);
        Self {
            inner: Arc::new(Inner { data }),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.data.is_empty()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        match self.inner.data.binary_search_by(|(k, _)| k.cmp(key)) {
            Ok(idx) => Some(&self.inner.data[idx].1),
            Err(_) => None,
        }
    }

    /// All `(K, V)` pairs in ascending key order.
    pub fn iter(&self) -> std::slice::Iter<'_, (K, V)> {
        self.inner.data.iter()
    }

    /// Sorted range over `[from, to]` (both inclusive). Returns a
    /// slice iterator over the snapshot's backing vec - no allocation.
    pub fn range(&self, from: &K, to: &K) -> std::slice::Iter<'_, (K, V)> {
        let lo = self.inner.data.partition_point(|(k, _)| k < from);
        let hi = self.inner.data.partition_point(|(k, _)| k <= to);
        self.inner.data[lo..hi].iter()
    }
}

impl<K, V> Clone for TreapSnapshot<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn collect<K: Clone, V: Clone>(treap: &Treap<K, V>, idx: u32, out: &mut Vec<(K, V)>) {
    if idx == NIL {
        return;
    }
    let node = &treap.nodes[idx as usize];
    collect(treap, node.left, out);
    out.push(((*node.key).clone(), (*node.value).clone()));
    collect(treap, node.right, out);
}

#[cfg(test)]
#[path = "concurrent_reads_tests.rs"]
mod tests;
