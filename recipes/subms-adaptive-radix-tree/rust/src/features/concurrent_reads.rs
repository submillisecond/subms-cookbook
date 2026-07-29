//! Read-only snapshot view.
//!
//! `ArtSnapshot<V>` is a frozen, immutable copy of the keyed entries at
//! the moment of snapshot. Readers can hold it across thread boundaries
//! and across mutations of the original tree. The snapshot is cheap to
//! query (`O(log n)` binary search on a sorted vector) and bounded in
//! size: it allocates one entry per keyed value, not one per tree node.
//!
//! Trade-off vs a pointer-versioned snapshot: this copies values, so V
//! must be `Clone`. For tiny values (u32, u64) the copy is the same
//! size as the pointer anyway; for large values, wrap them in `Arc`.

use std::sync::Arc;

use crate::{Art, Node};

#[derive(Clone)]
pub struct ArtSnapshot<V: Clone> {
    entries: Arc<Vec<(Vec<u8>, V)>>,
}

impl<V: Clone> ArtSnapshot<V> {
    /// Freeze the tree's keyed entries into a snapshot. Walks the tree
    /// once, in-order, copying each (key, value) pair.
    pub fn from_tree(tree: &Art<V>) -> Self {
        let mut entries: Vec<(Vec<u8>, V)> = Vec::new();
        let mut prefix: Vec<u8> = Vec::new();
        collect(tree.root(), &mut prefix, &mut entries);
        Self {
            entries: Arc::new(entries),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &[u8]) -> Option<&V> {
        let entries = self.entries.as_ref();
        match entries.binary_search_by(|(k, _)| k.as_slice().cmp(key)) {
            Ok(i) => Some(&entries[i].1),
            Err(_) => None,
        }
    }

    /// Iterate (key, value) pairs in byte-lex order.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &V)> {
        self.entries.iter().map(|(k, v)| (k.as_slice(), v))
    }
}

fn collect<V: Clone>(node: &Node<V>, prefix: &mut Vec<u8>, out: &mut Vec<(Vec<u8>, V)>) {
    // Path compression: the node's own prefix bytes precede its value + edges.
    prefix.extend_from_slice(&node.prefix);
    if let Some(v) = node.value.as_ref() {
        out.push((prefix.clone(), v.clone()));
    }
    for (byte, child) in node.children.sorted_pairs() {
        prefix.push(byte);
        collect(child, prefix, out);
        prefix.pop();
    }
    prefix.truncate(prefix.len() - node.prefix.len());
}

#[cfg(test)]
#[path = "concurrent_reads_tests.rs"]
mod tests;
