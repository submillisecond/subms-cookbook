//! Treap - probabilistic balanced BST.
//!
//! Each node carries a random priority. The tree is a BST on keys and a
//! max-heap on priorities. Insert + delete rebalance via tree rotations.
//! With uniform priorities the expected height is `O(log n)`.
//!
//! ```
//! use subms_treap::Treap;
//! let mut t: Treap<u32, &'static str> = Treap::new(42);
//! t.insert(3, "three");
//! t.insert(1, "one");
//! t.insert(2, "two");
//! assert_eq!(t.get(&2).copied(), Some("two"));
//! assert_eq!(t.len(), 3);
//! assert_eq!(t.remove(&1), Some("one"));
//! assert_eq!(t.len(), 2);
//! ```

use std::cmp::Ordering;

pub struct Treap<K, V> {
    root: Option<Box<Node<K, V>>>,
    len: usize,
    rng_state: u64,
}

struct Node<K, V> {
    key: K,
    value: V,
    priority: u64,
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
}

impl<K: Ord, V> Treap<K, V> {
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

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let (new_root, removed) = rem(self.root.take(), key);
        self.root = new_root;
        if removed.is_some() {
            self.len -= 1;
        }
        removed
    }

    /// In-order traversal; pushes `(key, value)` references into a Vec.
    pub fn collect_in_order(&self) -> Vec<(&K, &V)> {
        let mut out = Vec::with_capacity(self.len);
        in_order(self.root.as_deref(), &mut out);
        out
    }

    fn next_priority(&mut self) -> u64 {
        // LCG: same constants as subms::SubMsLcg so cookbook recipes share
        // the deterministic priority sequence at a given seed.
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng_state
    }
}

fn ins<K: Ord, V>(
    root: Option<Box<Node<K, V>>>,
    key: K,
    value: V,
    priority: u64,
) -> (Option<Box<Node<K, V>>>, Option<V>) {
    match root {
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
                if node
                    .left
                    .as_ref()
                    .map(|l| l.priority > node.priority)
                    .unwrap_or(false)
                {
                    node = rotate_right(node);
                }
                (Some(node), replaced)
            }
            Ordering::Greater => {
                let (new_right, replaced) = ins(node.right.take(), key, value, priority);
                node.right = new_right;
                if node
                    .right
                    .as_ref()
                    .map(|r| r.priority > node.priority)
                    .unwrap_or(false)
                {
                    node = rotate_left(node);
                }
                (Some(node), replaced)
            }
        },
    }
}

fn rem<K: Ord, V>(root: Option<Box<Node<K, V>>>, key: &K) -> (Option<Box<Node<K, V>>>, Option<V>) {
    match root {
        None => (None, None),
        Some(mut node) => match key.cmp(&node.key) {
            Ordering::Less => {
                let (new_left, removed) = rem(node.left.take(), key);
                node.left = new_left;
                (Some(node), removed)
            }
            Ordering::Greater => {
                let (new_right, removed) = rem(node.right.take(), key);
                node.right = new_right;
                (Some(node), removed)
            }
            Ordering::Equal => {
                let value = node.value;
                let merged = merge_subtrees(node.left, node.right);
                (merged, Some(value))
            }
        },
    }
}

fn merge_subtrees<K: Ord, V>(
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
) -> Option<Box<Node<K, V>>> {
    match (left, right) {
        (None, r) => r,
        (l, None) => l,
        (Some(mut l), Some(mut r)) => {
            if l.priority > r.priority {
                l.right = merge_subtrees(l.right.take(), Some(r));
                Some(l)
            } else {
                r.left = merge_subtrees(Some(l), r.left.take());
                Some(r)
            }
        }
    }
}

fn rotate_right<K, V>(mut node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut left = node.left.take().expect("rotate_right requires left child");
    node.left = left.right.take();
    left.right = Some(node);
    left
}

fn rotate_left<K, V>(mut node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut right = node.right.take().expect("rotate_left requires right child");
    node.right = right.left.take();
    right.left = Some(node);
    right
}

fn in_order<'a, K, V>(node: Option<&'a Node<K, V>>, out: &mut Vec<(&'a K, &'a V)>) {
    if let Some(n) = node {
        in_order(n.left.as_deref(), out);
        out.push((&n.key, &n.value));
        in_order(n.right.as_deref(), out);
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
