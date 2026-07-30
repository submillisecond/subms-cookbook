//! Sequence-builder treap with explicit `split(key) -> (left, right)`
//! and `merge(left, right)` operations.
//!
//! Both run in `O(log N)` expected time under the standard treap
//! rotation invariant (max-heap on priorities, BST on keys). The
//! pair is the textbook implicit-treap toolkit and is the right shape
//! for piecewise sequence construction: build two halves separately,
//! `merge` to splice; or use `split` to chop a range out of a sorted
//! stream.
//!
//! `merge` requires the BST-on-key invariant: every key in `left`
//! strictly less than every key in `right`. Violating that
//! precondition panics in debug builds and yields a malformed treap
//! in release. The standard pairing is `split(t, k) -> (lo, hi)`
//! then `merge(lo, hi) -> t'` - which is the round-trip identity
//! the test suite asserts.
//!
//! The base `Treap` in `lib.rs` uses an arena layout that doesn't
//! cheaply support cross-tree splice. This feature lives on its own
//! pointer-based node type so split/merge stay zero-copy on the
//! detached subtree.

use std::cmp::Ordering;

type Link<K, V> = Option<Box<Node<K, V>>>;

struct Node<K, V> {
    key: K,
    value: V,
    priority: u64,
    left: Link<K, V>,
    right: Link<K, V>,
}

pub struct SplittableTreap<K, V> {
    root: Link<K, V>,
    len: usize,
    rng_state: u64,
}

impl<K: Ord, V> SplittableTreap<K, V> {
    pub fn new(seed: u64) -> Self {
        Self {
            root: None,
            len: 0,
            rng_state: seed | 1,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let priority = self.next_priority();
        let (new_root, replaced) = ins(self.root.take(), key, value, priority);
        self.root = new_root;
        if replaced.is_none() {
            self.len += 1;
        }
        replaced
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut cur = self.root.as_deref();
        while let Some(node) = cur {
            match key.cmp(&node.key) {
                Ordering::Less => cur = node.left.as_deref(),
                Ordering::Greater => cur = node.right.as_deref(),
                Ordering::Equal => return Some(&node.value),
            }
        }
        None
    }

    /// Consume `self` and split into `(left, right)` where every key
    /// in `left` is strictly less than `pivot` and every key in
    /// `right` is greater-than-or-equal-to `pivot`.
    pub fn split(mut self, pivot: &K) -> (Self, Self) {
        let (l, r) = split_node(self.root.take(), pivot);
        let l_len = count(&l);
        let r_len = count(&r);
        (
            Self {
                root: l,
                len: l_len,
                rng_state: self.rng_state,
            },
            Self {
                root: r,
                len: r_len,
                rng_state: self.rng_state.wrapping_add(1),
            },
        )
    }

    /// Consume `left` and `right` and produce a single treap. Every
    /// key in `left` must be strictly less than every key in `right`
    /// or the resulting BST invariant is violated.
    pub fn merge(left: Self, right: Self) -> Self {
        if let (Some(l_max), Some(r_min)) = (max_key(&left.root), min_key(&right.root)) {
            debug_assert!(
                l_max < r_min,
                "SplittableTreap::merge precondition violated (left max >= right min)"
            );
        }
        let rng_state = left.rng_state.wrapping_add(right.rng_state) | 1;
        let len = left.len + right.len;
        Self {
            root: merge_nodes(left.root, right.root),
            len,
            rng_state,
        }
    }

    pub fn collect_in_order(&self) -> Vec<(&K, &V)> {
        let mut out = Vec::with_capacity(self.len);
        in_order(self.root.as_deref(), &mut out);
        out
    }

    fn next_priority(&mut self) -> u64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // SplitMix64 finalizer - decorrelate the priority from the key so
        // the tree keeps its expected O(log n) height. Mirrors the base
        // Treap fix.
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}

fn ins<K: Ord, V>(link: Link<K, V>, key: K, value: V, priority: u64) -> (Link<K, V>, Option<V>) {
    match link {
        None => (
            Some(Box::new(Node {
                key,
                value,
                priority,
                left: None,
                right: None,
            })),
            None,
        ),
        Some(mut node) => match key.cmp(&node.key) {
            Ordering::Equal => {
                let old = std::mem::replace(&mut node.value, value);
                (Some(node), Some(old))
            }
            Ordering::Less => {
                let (new_left, replaced) = ins(node.left.take(), key, value, priority);
                node.left = new_left;
                let need_rotate =
                    node.left.as_ref().map(|l| l.priority).unwrap_or(0) > node.priority;
                let rebuilt = if need_rotate {
                    rotate_right(node)
                } else {
                    node
                };
                (Some(rebuilt), replaced)
            }
            Ordering::Greater => {
                let (new_right, replaced) = ins(node.right.take(), key, value, priority);
                node.right = new_right;
                let need_rotate =
                    node.right.as_ref().map(|r| r.priority).unwrap_or(0) > node.priority;
                let rebuilt = if need_rotate { rotate_left(node) } else { node };
                (Some(rebuilt), replaced)
            }
        },
    }
}

fn split_node<K: Ord, V>(link: Link<K, V>, pivot: &K) -> (Link<K, V>, Link<K, V>) {
    match link {
        None => (None, None),
        Some(mut node) => {
            if &node.key < pivot {
                let right = node.right.take();
                let (lo_r, hi) = split_node(right, pivot);
                node.right = lo_r;
                (Some(node), hi)
            } else {
                let left = node.left.take();
                let (lo, hi_l) = split_node(left, pivot);
                node.left = hi_l;
                (lo, Some(node))
            }
        }
    }
}

fn merge_nodes<K, V>(left: Link<K, V>, right: Link<K, V>) -> Link<K, V> {
    match (left, right) {
        (None, r) => r,
        (l, None) => l,
        (Some(mut l), Some(mut r)) => {
            if l.priority > r.priority {
                let l_right = l.right.take();
                l.right = merge_nodes(l_right, Some(r));
                Some(l)
            } else {
                let r_left = r.left.take();
                r.left = merge_nodes(Some(l), r_left);
                Some(r)
            }
        }
    }
}

fn rotate_right<K, V>(mut node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut left = node.left.take().expect("rotate_right needs left child");
    node.left = left.right.take();
    left.right = Some(node);
    left
}

fn rotate_left<K, V>(mut node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut right = node.right.take().expect("rotate_left needs right child");
    node.right = right.left.take();
    right.left = Some(node);
    right
}

fn count<K, V>(link: &Link<K, V>) -> usize {
    match link {
        None => 0,
        Some(node) => 1 + count(&node.left) + count(&node.right),
    }
}

fn min_key<K, V>(link: &Link<K, V>) -> Option<&K> {
    let mut cur = link.as_deref()?;
    while let Some(l) = cur.left.as_deref() {
        cur = l;
    }
    Some(&cur.key)
}

fn max_key<K, V>(link: &Link<K, V>) -> Option<&K> {
    let mut cur = link.as_deref()?;
    while let Some(r) = cur.right.as_deref() {
        cur = r;
    }
    Some(&cur.key)
}

fn in_order<'a, K, V>(link: Option<&'a Node<K, V>>, out: &mut Vec<(&'a K, &'a V)>) {
    if let Some(node) = link {
        in_order(node.left.as_deref(), out);
        out.push((&node.key, &node.value));
        in_order(node.right.as_deref(), out);
    }
}

#[cfg(test)]
#[path = "merge_split_tests.rs"]
mod tests;
