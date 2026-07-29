//! Treap - probabilistic balanced BST.
//!
//! Each node carries a random priority. The tree is a BST on keys and a
//! max-heap on priorities. Insert + delete rebalance via tree rotations.
//! With uniform priorities the expected height is `O(log n)`.
//!
//! Nodes are stored in a contiguous `Vec<Node>` and referenced by `u32`
//! indices (NULL = `u32::MAX`). This is the production-style memory
//! layout: avoids one heap allocation per insert (the `Box::new(Node)`
//! pattern), keeps nodes cache-dense, and lets the tree resize via
//! `Vec::push` amortised O(1) instead of fragmenting the global heap.
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

pub(crate) const NIL: u32 = u32::MAX;

pub struct Treap<K, V> {
    pub(crate) nodes: Vec<Node<K, V>>,
    // Singly-linked free list. Head index, or NIL when empty.
    // Reuses slots vacated by `remove()` so the Vec stops growing under
    // an insert/remove churn workload.
    free_head: u32,
    pub(crate) root: u32,
    len: usize,
    rng_state: u64,
}

pub(crate) struct Node<K, V> {
    pub(crate) key: K,
    pub(crate) value: V,
    pub(crate) priority: u64,
    pub(crate) left: u32,
    pub(crate) right: u32,
}

impl<K: Ord, V> Treap<K, V> {
    pub fn new(seed: u64) -> Self {
        Self {
            nodes: Vec::new(),
            free_head: NIL,
            root: NIL,
            len: 0,
            rng_state: seed | 1,
        }
    }

    /// Construct with capacity pre-allocated. Use when an upper bound on
    /// the working set is known: avoids the doubling-vec growth path
    /// during the first burst of inserts.
    pub fn with_capacity(seed: u64, capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
            free_head: NIL,
            root: NIL,
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
        let (new_root, replaced) = self.ins(self.root, key, value, priority);
        self.root = new_root;
        if replaced.is_none() {
            self.len += 1;
        }
        replaced
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut cur = self.root;
        while cur != NIL {
            let node = &self.nodes[cur as usize];
            match key.cmp(&node.key) {
                Ordering::Less => cur = node.left,
                Ordering::Greater => cur = node.right,
                Ordering::Equal => return Some(&node.value),
            }
        }
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let (new_root, removed) = self.rem(self.root, key);
        self.root = new_root;
        if removed.is_some() {
            self.len -= 1;
        }
        removed
    }

    /// In-order traversal; pushes `(key, value)` references into a Vec.
    pub fn collect_in_order(&self) -> Vec<(&K, &V)> {
        let mut out = Vec::with_capacity(self.len);
        self.in_order(self.root, &mut out);
        out
    }

    fn next_priority(&mut self) -> u64 {
        // LCG step: same constants as subms::SubMsLcg.
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // SplitMix64 finalizer. The bare LCG state stays correlated with
        // any sibling LCG-derived stream - including keys generated from
        // the same family of constants - and a priority correlated with
        // the key sorts the treap into a spine (O(n) depth). The avalanche
        // decorrelates the priority from the key so the heap invariant
        // produces the random shape the O(log n) bound assumes. Same
        // fix the hyperloglog recipe applies to FNV-1a output.
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn alloc(&mut self, key: K, value: V, priority: u64) -> u32 {
        if self.free_head != NIL {
            let idx = self.free_head;
            let slot = &mut self.nodes[idx as usize];
            // free-list link was stored in `left` while the slot was free
            self.free_head = slot.left;
            slot.key = key;
            slot.value = value;
            slot.priority = priority;
            slot.left = NIL;
            slot.right = NIL;
            idx
        } else {
            let idx = self.nodes.len() as u32;
            self.nodes.push(Node {
                key,
                value,
                priority,
                left: NIL,
                right: NIL,
            });
            idx
        }
    }

    fn free(&mut self, idx: u32) {
        self.nodes[idx as usize].left = self.free_head;
        self.free_head = idx;
    }

    fn ins(&mut self, root: u32, key: K, value: V, priority: u64) -> (u32, Option<V>) {
        if root == NIL {
            return (self.alloc(key, value, priority), None);
        }
        let cmp = key.cmp(&self.nodes[root as usize].key);
        match cmp {
            Ordering::Equal => {
                let old = std::mem::replace(&mut self.nodes[root as usize].value, value);
                (root, Some(old))
            }
            Ordering::Less => {
                let left = self.nodes[root as usize].left;
                let (new_left, replaced) = self.ins(left, key, value, priority);
                self.nodes[root as usize].left = new_left;
                let new_left_pri = self.nodes[new_left as usize].priority;
                let root_pri = self.nodes[root as usize].priority;
                let r = if new_left_pri > root_pri {
                    self.rotate_right(root)
                } else {
                    root
                };
                (r, replaced)
            }
            Ordering::Greater => {
                let right = self.nodes[root as usize].right;
                let (new_right, replaced) = self.ins(right, key, value, priority);
                self.nodes[root as usize].right = new_right;
                let new_right_pri = self.nodes[new_right as usize].priority;
                let root_pri = self.nodes[root as usize].priority;
                let r = if new_right_pri > root_pri {
                    self.rotate_left(root)
                } else {
                    root
                };
                (r, replaced)
            }
        }
    }

    fn rem(&mut self, root: u32, key: &K) -> (u32, Option<V>) {
        if root == NIL {
            return (NIL, None);
        }
        let cmp = key.cmp(&self.nodes[root as usize].key);
        match cmp {
            Ordering::Less => {
                let left = self.nodes[root as usize].left;
                let (new_left, removed) = self.rem(left, key);
                self.nodes[root as usize].left = new_left;
                (root, removed)
            }
            Ordering::Greater => {
                let right = self.nodes[root as usize].right;
                let (new_right, removed) = self.rem(right, key);
                self.nodes[root as usize].right = new_right;
                (root, removed)
            }
            Ordering::Equal => {
                let left = self.nodes[root as usize].left;
                let right = self.nodes[root as usize].right;
                let value = unsafe {
                    // value moved out; the slot is about to go on the
                    // free list - safe because `free` doesn't read it.
                    std::ptr::read(&self.nodes[root as usize].value)
                };
                let merged = self.merge_subtrees(left, right);
                self.free(root);
                (merged, Some(value))
            }
        }
    }

    fn merge_subtrees(&mut self, left: u32, right: u32) -> u32 {
        if left == NIL {
            return right;
        }
        if right == NIL {
            return left;
        }
        let l_pri = self.nodes[left as usize].priority;
        let r_pri = self.nodes[right as usize].priority;
        if l_pri > r_pri {
            let l_right = self.nodes[left as usize].right;
            let merged = self.merge_subtrees(l_right, right);
            self.nodes[left as usize].right = merged;
            left
        } else {
            let r_left = self.nodes[right as usize].left;
            let merged = self.merge_subtrees(left, r_left);
            self.nodes[right as usize].left = merged;
            right
        }
    }

    fn rotate_right(&mut self, idx: u32) -> u32 {
        let left = self.nodes[idx as usize].left;
        debug_assert!(left != NIL, "rotate_right requires left child");
        let left_right = self.nodes[left as usize].right;
        self.nodes[idx as usize].left = left_right;
        self.nodes[left as usize].right = idx;
        left
    }

    fn rotate_left(&mut self, idx: u32) -> u32 {
        let right = self.nodes[idx as usize].right;
        debug_assert!(right != NIL, "rotate_left requires right child");
        let right_left = self.nodes[right as usize].left;
        self.nodes[idx as usize].right = right_left;
        self.nodes[right as usize].left = idx;
        right
    }

    fn in_order<'a>(&'a self, idx: u32, out: &mut Vec<(&'a K, &'a V)>) {
        if idx == NIL {
            return;
        }
        let node = &self.nodes[idx as usize];
        self.in_order(node.left, out);
        out.push((&node.key, &node.value));
        self.in_order(node.right, out);
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each submodule is gated by its own Cargo
// feature flag. See `Cargo.toml` `[features]` and the cookbook page
// for per-feature semantics + p99 impact.
#[cfg(any(
    feature = "range-query",
    feature = "persistent",
    feature = "merge-split",
    feature = "concurrent-reads",
))]
pub mod features;

#[cfg(feature = "concurrent-reads")]
pub use features::concurrent_reads::TreapSnapshot;
#[cfg(feature = "merge-split")]
pub use features::merge_split::SplittableTreap;
#[cfg(feature = "persistent")]
pub use features::persistent::PersistentTreap;
#[cfg(feature = "range-query")]
pub use features::range_query::{RangeBound, RangeIter};

// Crate-level unit tests live in colocated files (org convention:
// `<module>_tests.rs` alongside the module), not the top-level `tests/` dir.
#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
