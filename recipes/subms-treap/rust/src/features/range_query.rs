//! Sorted-range iteration over a treap.
//!
//! `range(from, to)` yields every `(&K, &V)` whose key falls between
//! the bounds, in ascending key order. Bounds are either inclusive,
//! exclusive, or unbounded - mix freely.
//!
//! Iteration is stack-based (Morris-traversal-free) so a deep treap
//! still walks at the expected `O(log N)` peak stack depth. Each
//! `next()` is amortised `O(1)`; the full traversal of an N-key range
//! is `O(N)` plus `O(log T)` to locate the left boundary in a treap
//! of T total entries.
//!
//! The iterator borrows the treap immutably for its lifetime - it is
//! a "stable iteration over a snapshot" in the sense that the
//! compile-time borrow ensures no concurrent writer can mutate the
//! source while the iter is alive. Compose with `TreapSnapshot` from
//! the `concurrent-reads` feature when readers and writers cross
//! thread boundaries.

use crate::{NIL, Treap};

/// One end of a range query.
pub enum RangeBound<'a, K> {
    Unbounded,
    Inclusive(&'a K),
    Exclusive(&'a K),
}

impl<K: Ord, V> Treap<K, V> {
    /// Iterate every `(&K, &V)` with `from <= key <= to` (or whichever
    /// inclusion shape the bounds declare), in ascending key order.
    pub fn range<'a>(
        &'a self,
        from: RangeBound<'a, K>,
        to: RangeBound<'a, K>,
    ) -> RangeIter<'a, K, V> {
        let mut iter = RangeIter {
            treap: self,
            stack: Vec::new(),
            to,
        };
        iter.descend_to_lower_bound(self.root, &from);
        iter
    }
}

pub struct RangeIter<'a, K, V> {
    treap: &'a Treap<K, V>,
    /// In-order stack: ancestors with the left subtree consumed but
    /// the node itself not yet emitted.
    stack: Vec<u32>,
    to: RangeBound<'a, K>,
}

impl<'a, K: Ord, V> RangeIter<'a, K, V> {
    fn descend_to_lower_bound(&mut self, mut idx: u32, from: &RangeBound<'a, K>) {
        while idx != NIL {
            let node = &self.treap.nodes[idx as usize];
            let take_left = match from {
                RangeBound::Unbounded => true,
                RangeBound::Inclusive(k) => &node.key >= k,
                RangeBound::Exclusive(k) => &node.key > k,
            };
            if take_left {
                self.stack.push(idx);
                idx = node.left;
            } else {
                idx = node.right;
            }
        }
    }

    fn in_upper_bound(&self, key: &K) -> bool {
        match &self.to {
            RangeBound::Unbounded => true,
            RangeBound::Inclusive(k) => key <= k,
            RangeBound::Exclusive(k) => key < k,
        }
    }
}

impl<'a, K: Ord, V> Iterator for RangeIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.stack.pop()?;
        let node = &self.treap.nodes[idx as usize];
        if !self.in_upper_bound(&node.key) {
            self.stack.clear();
            return None;
        }
        let mut right = node.right;
        // Standard in-order: descend left from the right child, push the spine.
        while right != NIL {
            self.stack.push(right);
            right = self.treap.nodes[right as usize].left;
        }
        Some((&node.key, &node.value))
    }
}

#[cfg(test)]
#[path = "range_query_tests.rs"]
mod tests;
