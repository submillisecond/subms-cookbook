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
mod tests {
    use super::*;

    fn build_treap(keys: &[i32]) -> Treap<i32, i32> {
        let mut t: Treap<i32, i32> = Treap::new(42);
        for &k in keys {
            t.insert(k, k * 10);
        }
        t
    }

    fn collect_range<'a>(
        t: &'a Treap<i32, i32>,
        from: RangeBound<'a, i32>,
        to: RangeBound<'a, i32>,
    ) -> Vec<(i32, i32)> {
        t.range(from, to).map(|(k, v)| (*k, *v)).collect()
    }

    #[test]
    fn empty_treap_yields_nothing() {
        let t: Treap<i32, i32> = Treap::new(0);
        let out = collect_range(&t, RangeBound::Unbounded, RangeBound::Unbounded);
        assert!(out.is_empty());
    }

    #[test]
    fn single_node_inclusive_match() {
        let t = build_treap(&[5]);
        let out = collect_range(&t, RangeBound::Inclusive(&5), RangeBound::Inclusive(&5));
        assert_eq!(out, vec![(5, 50)]);
    }

    #[test]
    fn single_node_exclusive_misses() {
        let t = build_treap(&[5]);
        let out = collect_range(&t, RangeBound::Exclusive(&5), RangeBound::Inclusive(&100));
        assert!(out.is_empty());
    }

    #[test]
    fn inclusive_bounds_yield_sorted_window() {
        let t = build_treap(&[5, 1, 9, 3, 7, 2, 8, 4, 6]);
        let out = collect_range(&t, RangeBound::Inclusive(&3), RangeBound::Inclusive(&7));
        let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn exclusive_bounds_drop_endpoints() {
        let t = build_treap(&[5, 1, 9, 3, 7, 2, 8, 4, 6]);
        let out = collect_range(&t, RangeBound::Exclusive(&3), RangeBound::Exclusive(&7));
        let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![4, 5, 6]);
    }

    #[test]
    fn unbounded_below_iterates_from_min() {
        let t = build_treap(&[5, 1, 9, 3, 7]);
        let out = collect_range(&t, RangeBound::Unbounded, RangeBound::Inclusive(&5));
        let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![1, 3, 5]);
    }

    #[test]
    fn unbounded_above_iterates_to_max() {
        let t = build_treap(&[5, 1, 9, 3, 7]);
        let out = collect_range(&t, RangeBound::Inclusive(&5), RangeBound::Unbounded);
        let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![5, 7, 9]);
    }

    #[test]
    fn range_outside_keys_yields_nothing() {
        let t = build_treap(&[10, 20, 30]);
        let out = collect_range(&t, RangeBound::Inclusive(&100), RangeBound::Inclusive(&200));
        assert!(out.is_empty());
    }

    #[test]
    fn values_match_keys_in_range() {
        let t = build_treap(&[1, 2, 3, 4, 5]);
        let out = collect_range(&t, RangeBound::Inclusive(&2), RangeBound::Inclusive(&4));
        assert_eq!(out, vec![(2, 20), (3, 30), (4, 40)]);
    }

    #[test]
    fn large_treap_in_order_invariant() {
        let mut t: Treap<i32, i32> = Treap::new(99);
        for i in 0..1_000 {
            t.insert(i, i);
        }
        let out: Vec<i32> = t
            .range(RangeBound::Inclusive(&100), RangeBound::Inclusive(&899))
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(out.len(), 800);
        for w in out.windows(2) {
            assert!(w[0] < w[1]);
        }
        assert_eq!(*out.first().unwrap(), 100);
        assert_eq!(*out.last().unwrap(), 899);
    }
}
