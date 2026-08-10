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
//!
//! // Ordered navigation, no feature flag needed.
//! assert_eq!(t.first().map(|(k, _)| *k), Some(2));
//! assert_eq!(t.ceiling(&3).map(|(k, _)| *k), Some(3));
//! assert_eq!(t.predecessor(&3).map(|(k, _)| *k), Some(2));
//! assert_eq!(t.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![2, 3]);
//! ```
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-treap>

use std::cmp::Ordering;
use std::fmt;
use std::mem::ManuallyDrop;

pub(crate) const NIL: u32 = u32::MAX;

// Parked in `right` on a vacated slot. `Drop` and `clear` need to tell a slot
// whose payload has already been moved out from a live one, and the arena can
// never reach this index: `u32::MAX - 1` nodes is far past the address space a
// `Vec<Node>` can hold.
const FREE: u32 = u32::MAX - 1;

/// The one fallible operation's error.
///
/// Every other method on `Treap` is total: lookups return `Option`, removals
/// of an absent key are a no-op, and there is no capacity to exhaust short of
/// the allocator failing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TreapError {
    /// `from_sorted` received input that is not strictly ascending. `index` is
    /// the position of the offending item.
    UnsortedInput { index: usize },
    /// `join` was handed two treaps whose key ranges overlap. Joining them
    /// would break the BST invariant, so the operation is refused and both
    /// treaps are left untouched.
    OverlappingRange,
}

impl fmt::Display for TreapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreapError::UnsortedInput { index } => {
                write!(
                    f,
                    "from_sorted input not strictly ascending at index {index}"
                )
            }
            TreapError::OverlappingRange => {
                write!(
                    f,
                    "join requires every key on the left below every key on the right"
                )
            }
        }
    }
}

impl std::error::Error for TreapError {}

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
    // ManuallyDrop is what makes slot reuse sound: a vacated slot has had its
    // payload moved out, so assigning over it must not run the old value's
    // destructor. `Drop for Treap` then drops exactly the live slots. Layout
    // is identical to a bare `K`/`V`, so the arena stays as dense as it looks.
    pub(crate) key: ManuallyDrop<K>,
    pub(crate) value: ManuallyDrop<V>,
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

    /// Seed the priority stream from the OS rather than from a constant.
    ///
    /// The default constructor takes an explicit seed because a reproducible
    /// tree shape is what makes a benchmark and a bug report mean anything.
    /// That same property is a liability when an attacker can both choose the
    /// keys and observe the latency: the priority sequence is then known, and
    /// a chosen key order can force the spine the randomized bound rules out.
    /// Reach for this when the key stream is untrusted, and accept that two
    /// runs no longer produce the same tree.
    pub fn from_entropy() -> Self {
        use std::hash::{BuildHasher, Hasher, RandomState};
        // std has no RNG, but RandomState is seeded by the OS per instance,
        // which is exactly the one bit of entropy needed here.
        Self::new(RandomState::new().build_hasher().finish())
    }

    /// Build from already-sorted input in `O(n)`, skipping the `n` rotating
    /// inserts a naive rebuild would pay.
    ///
    /// Keys must be strictly ascending; duplicates are rejected rather than
    /// collapsed, because silently dropping one of two entries is the wrong
    /// answer for every workload that reaches for this. Pairs with
    /// [`Treap::collect_in_order`] as a snapshot / restore round trip.
    ///
    /// ```
    /// use subms_treap::Treap;
    /// let t = Treap::from_sorted(1, [(1u32, "a"), (2, "b"), (3, "c")]).unwrap();
    /// assert_eq!(t.len(), 3);
    /// assert_eq!(t.get(&2).copied(), Some("b"));
    /// ```
    pub fn from_sorted(
        seed: u64,
        items: impl IntoIterator<Item = (K, V)>,
    ) -> Result<Self, TreapError> {
        let iter = items.into_iter();
        let mut t = Self::with_capacity(seed, iter.size_hint().0);
        // Right spine of the tree built so far, priorities descending from the
        // root. Every new key exceeds everything already placed, so it can only
        // enter along that spine - which is the Cartesian-tree construction.
        let mut spine: Vec<u32> = Vec::new();
        for (index, (key, value)) in iter.enumerate() {
            if let Some(&prev) = spine.last()
                && t.key_at(prev) >= &key
            {
                return Err(TreapError::UnsortedInput { index });
            }
            let priority = t.next_priority();
            let idx = t.alloc(key, value, priority);
            let mut demoted = NIL;
            while let Some(&top) = spine.last() {
                if t.nodes[top as usize].priority < priority {
                    demoted = spine.pop().unwrap();
                } else {
                    break;
                }
            }
            t.nodes[idx as usize].left = demoted;
            match spine.last() {
                Some(&top) => t.nodes[top as usize].right = idx,
                None => t.root = idx,
            }
            spine.push(idx);
            t.len += 1;
        }
        Ok(t)
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Longest root-to-leaf path in edges; `0` for an empty or single-node
    /// tree. The randomized-priority bound puts this near `3 * ln(n)` in
    /// expectation, so it is the cheapest way to see whether the priority
    /// stream is doing its job on real keys.
    pub fn height(&self) -> usize {
        let mut best = 0;
        let mut stack = vec![(self.root, 0usize)];
        while let Some((idx, depth)) = stack.pop() {
            if idx == NIL {
                continue;
            }
            best = best.max(depth);
            let node = &self.nodes[idx as usize];
            stack.push((node.left, depth + 1));
            stack.push((node.right, depth + 1));
        }
        best
    }

    /// Drop every entry and reset to empty, keeping the arena's capacity so a
    /// rebuild does not pay the growth path again.
    pub fn clear(&mut self) {
        self.drop_live_payloads();
        self.nodes.clear();
        self.free_head = NIL;
        self.root = NIL;
        self.len = 0;
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
        let idx = self.find(key);
        (idx != NIL).then(|| &*self.nodes[idx as usize].value)
    }

    /// Mutable access to a resting value. The amend path for a price level:
    /// no re-descent through `insert`, no priority redraw, no rotation.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let idx = self.find(key);
        (idx != NIL).then(|| &mut *self.nodes[idx as usize].value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.find(key) != NIL
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let (new_root, removed) = self.rem(self.root, key);
        self.root = new_root;
        if removed.is_some() {
            self.len -= 1;
        }
        removed
    }

    /// Smallest key and its value.
    pub fn first(&self) -> Option<(&K, &V)> {
        self.spine_end(false).map(|idx| self.entry_at(idx))
    }

    /// Largest key and its value.
    pub fn last(&self) -> Option<(&K, &V)> {
        self.spine_end(true).map(|idx| self.entry_at(idx))
    }

    /// Greatest key `<= key`.
    pub fn floor(&self, key: &K) -> Option<(&K, &V)> {
        self.search_le(key, false).map(|idx| self.entry_at(idx))
    }

    /// Least key `>= key`.
    pub fn ceiling(&self, key: &K) -> Option<(&K, &V)> {
        self.search_ge(key, false).map(|idx| self.entry_at(idx))
    }

    /// Greatest key strictly `< key`.
    pub fn predecessor(&self, key: &K) -> Option<(&K, &V)> {
        self.search_le(key, true).map(|idx| self.entry_at(idx))
    }

    /// Least key strictly `> key`.
    pub fn successor(&self, key: &K) -> Option<(&K, &V)> {
        self.search_ge(key, true).map(|idx| self.entry_at(idx))
    }

    /// Remove and return the smallest entry. The top-of-book sweep.
    pub fn pop_first(&mut self) -> Option<(K, V)> {
        let (new_root, popped) = self.pop_extreme(self.root, false);
        self.root = new_root;
        if popped.is_some() {
            self.len -= 1;
        }
        popped
    }

    /// Remove and return the largest entry.
    pub fn pop_last(&mut self) -> Option<(K, V)> {
        let (new_root, popped) = self.pop_extreme(self.root, true);
        self.root = new_root;
        if popped.is_some() {
            self.len -= 1;
        }
        popped
    }

    /// Cut the treap at `pivot`, keeping everything below it and returning
    /// everything at or above it.
    ///
    /// The cut itself is the treap's distinguishing operation against a
    /// red-black tree: one descent, expected `O(log n)`, no rebalancing pass.
    /// The arena then charges for what it buys elsewhere - the upper half's
    /// `m` nodes are relocated into their own arena, so the whole call is
    /// expected `O(log n) + O(m)`. Where that relocation matters, the
    /// `merge-split` feature's `SplittableTreap` is the pointer-backed variant
    /// that hands the detached subtree over without touching it.
    ///
    /// ```
    /// use subms_treap::Treap;
    /// let mut book: Treap<u32, u64> = Treap::new(7);
    /// for px in [9998u32, 9999, 10_000, 10_001] { book.insert(px, 100); }
    /// let marketable = book.split_off(&10_000);
    /// assert_eq!(book.len(), 2);
    /// assert_eq!(marketable.len(), 2);
    /// assert_eq!(marketable.first().map(|(k, _)| *k), Some(10_000));
    /// ```
    pub fn split_off(&mut self, pivot: &K) -> Self {
        let (lo, hi) = self.split_node(self.root, pivot);
        self.root = lo;
        let mut upper = Self::new(self.rng_state ^ 0x9e3779b97f4a7c15);
        let (new_root, moved) = upper.absorb_subtree(self, hi);
        upper.root = new_root;
        upper.len = moved;
        self.len -= moved;
        upper
    }

    /// Splice `other` onto the end of `self`. Every key in `self` must be
    /// strictly below every key in `other`.
    ///
    /// The splice is expected `O(log n)`; as with [`Treap::split_off`], moving
    /// `other`'s `m` nodes into this arena adds `O(m)`. An overlapping range is
    /// refused rather than silently corrupting the BST invariant, and both
    /// treaps are left as they were.
    pub fn join(&mut self, mut other: Self) -> Result<(), TreapError> {
        let overlaps = match (self.last(), other.first()) {
            (Some((l, _)), Some((r, _))) => l >= r,
            _ => false,
        };
        if overlaps {
            return Err(TreapError::OverlappingRange);
        }
        let other_root = other.root;
        let (moved_root, moved) = self.absorb_subtree(&mut other, other_root);
        other.root = NIL;
        other.len = 0;
        self.root = self.merge_subtrees(self.root, moved_root);
        self.len += moved;
        Ok(())
    }

    /// Ascending in-order iteration. Lazy: the only allocation is the
    /// traversal stack, sized to the tree's height.
    pub fn iter(&self) -> Iter<'_, K, V> {
        let mut it = Iter {
            treap: self,
            stack: Vec::new(),
        };
        it.push_left(self.root);
        it
    }

    /// Descending in-order iteration. A bid ladder is read best price first,
    /// which is the reverse of the stored order.
    pub fn iter_rev(&self) -> IterRev<'_, K, V> {
        let mut it = IterRev {
            treap: self,
            stack: Vec::new(),
        };
        it.push_right(self.root);
        it
    }

    /// In-order traversal; pushes `(key, value)` references into a Vec.
    pub fn collect_in_order(&self) -> Vec<(&K, &V)> {
        self.iter().collect()
    }

    fn find(&self, key: &K) -> u32 {
        let mut cur = self.root;
        while cur != NIL {
            let node = &self.nodes[cur as usize];
            match key.cmp(&node.key) {
                Ordering::Less => cur = node.left,
                Ordering::Greater => cur = node.right,
                Ordering::Equal => return cur,
            }
        }
        NIL
    }

    fn spine_end(&self, rightmost: bool) -> Option<u32> {
        let mut cur = self.root;
        if cur == NIL {
            return None;
        }
        loop {
            let node = &self.nodes[cur as usize];
            let next = if rightmost { node.right } else { node.left };
            if next == NIL {
                return Some(cur);
            }
            cur = next;
        }
    }

    fn search_le(&self, key: &K, strict: bool) -> Option<u32> {
        let mut cur = self.root;
        let mut best = NIL;
        while cur != NIL {
            let node = &self.nodes[cur as usize];
            let ok = if strict {
                *node.key < *key
            } else {
                *node.key <= *key
            };
            if ok {
                best = cur;
                cur = node.right;
            } else {
                cur = node.left;
            }
        }
        (best != NIL).then_some(best)
    }

    fn search_ge(&self, key: &K, strict: bool) -> Option<u32> {
        let mut cur = self.root;
        let mut best = NIL;
        while cur != NIL {
            let node = &self.nodes[cur as usize];
            let ok = if strict {
                *node.key > *key
            } else {
                *node.key >= *key
            };
            if ok {
                best = cur;
                cur = node.left;
            } else {
                cur = node.right;
            }
        }
        (best != NIL).then_some(best)
    }

    fn entry_at(&self, idx: u32) -> (&K, &V) {
        let node = &self.nodes[idx as usize];
        (&node.key, &node.value)
    }

    pub(crate) fn key_at(&self, idx: u32) -> &K {
        &self.nodes[idx as usize].key
    }

    fn pop_extreme(&mut self, root: u32, rightmost: bool) -> (u32, Option<(K, V)>) {
        if root == NIL {
            return (NIL, None);
        }
        let node = &self.nodes[root as usize];
        let next = if rightmost { node.right } else { node.left };
        if next == NIL {
            let other = if rightmost { node.left } else { node.right };
            let payload = self.take_payload(root);
            self.free(root);
            return (other, Some(payload));
        }
        let (new_child, popped) = self.pop_extreme(next, rightmost);
        if rightmost {
            self.nodes[root as usize].right = new_child;
        } else {
            self.nodes[root as usize].left = new_child;
        }
        (root, popped)
    }

    /// Partition the subtree at `idx` into keys below `pivot` and keys at or
    /// above it. One descent, no rebalancing: the heap invariant survives
    /// because neither half ever gains an ancestor it did not already have.
    fn split_node(&mut self, idx: u32, pivot: &K) -> (u32, u32) {
        if idx == NIL {
            return (NIL, NIL);
        }
        if self.key_at(idx) < pivot {
            let right = self.nodes[idx as usize].right;
            let (lo_right, hi) = self.split_node(right, pivot);
            self.nodes[idx as usize].right = lo_right;
            (idx, hi)
        } else {
            let left = self.nodes[idx as usize].left;
            let (lo, hi_left) = self.split_node(left, pivot);
            self.nodes[idx as usize].left = hi_left;
            (lo, idx)
        }
    }

    /// Move a subtree out of `src`'s arena and into this one, priorities and
    /// shape intact. Returns the new root and the node count.
    fn absorb_subtree(&mut self, src: &mut Self, idx: u32) -> (u32, usize) {
        if idx == NIL {
            return (NIL, 0);
        }
        let (left, right, priority) = {
            let node = &src.nodes[idx as usize];
            (node.left, node.right, node.priority)
        };
        let (new_left, left_n) = self.absorb_subtree(src, left);
        let (new_right, right_n) = self.absorb_subtree(src, right);
        let (key, value) = src.take_payload(idx);
        src.free(idx);
        let new_idx = self.alloc(key, value, priority);
        self.nodes[new_idx as usize].left = new_left;
        self.nodes[new_idx as usize].right = new_right;
        (new_idx, left_n + right_n + 1)
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
            slot.key = ManuallyDrop::new(key);
            slot.value = ManuallyDrop::new(value);
            slot.priority = priority;
            slot.left = NIL;
            slot.right = NIL;
            idx
        } else {
            let idx = self.nodes.len() as u32;
            self.nodes.push(Node {
                key: ManuallyDrop::new(key),
                value: ManuallyDrop::new(value),
                priority,
                left: NIL,
                right: NIL,
            });
            idx
        }
    }

    fn take_payload(&mut self, idx: u32) -> (K, V) {
        let node = &mut self.nodes[idx as usize];
        unsafe {
            (
                ManuallyDrop::take(&mut node.key),
                ManuallyDrop::take(&mut node.value),
            )
        }
    }

    fn free(&mut self, idx: u32) {
        let head = self.free_head;
        let node = &mut self.nodes[idx as usize];
        node.left = head;
        node.right = FREE;
        self.free_head = idx;
    }

    fn drop_live_payloads(&mut self) {
        for node in &mut self.nodes {
            if node.right != FREE {
                unsafe {
                    ManuallyDrop::drop(&mut node.key);
                    ManuallyDrop::drop(&mut node.value);
                }
            }
        }
    }

    fn ins(&mut self, root: u32, key: K, value: V, priority: u64) -> (u32, Option<V>) {
        if root == NIL {
            return (self.alloc(key, value, priority), None);
        }
        let cmp = key.cmp(&self.nodes[root as usize].key);
        match cmp {
            Ordering::Equal => {
                let old = std::mem::replace(
                    &mut self.nodes[root as usize].value,
                    ManuallyDrop::new(value),
                );
                (root, Some(ManuallyDrop::into_inner(old)))
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
                let (key, value) = self.take_payload(root);
                drop(key);
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
}

impl<K, V> Drop for Treap<K, V> {
    fn drop(&mut self) {
        for node in &mut self.nodes {
            if node.right != FREE {
                unsafe {
                    ManuallyDrop::drop(&mut node.key);
                    ManuallyDrop::drop(&mut node.value);
                }
            }
        }
    }
}

impl<K: Ord + fmt::Debug, V: fmt::Debug> fmt::Debug for Treap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<'a, K: Ord, V> IntoIterator for &'a Treap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Ascending in-order iterator. See [`Treap::iter`].
pub struct Iter<'a, K, V> {
    treap: &'a Treap<K, V>,
    stack: Vec<u32>,
}

impl<K, V> Iter<'_, K, V> {
    fn push_left(&mut self, mut idx: u32) {
        while idx != NIL {
            self.stack.push(idx);
            idx = self.treap.nodes[idx as usize].left;
        }
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.stack.pop()?;
        let node = &self.treap.nodes[idx as usize];
        self.push_left(node.right);
        Some((&node.key, &node.value))
    }
}

/// Descending in-order iterator. See [`Treap::iter_rev`].
pub struct IterRev<'a, K, V> {
    treap: &'a Treap<K, V>,
    stack: Vec<u32>,
}

impl<K, V> IterRev<'_, K, V> {
    fn push_right(&mut self, mut idx: u32) {
        while idx != NIL {
            self.stack.push(idx);
            idx = self.treap.nodes[idx as usize].right;
        }
    }
}

impl<'a, K, V> Iterator for IterRev<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.stack.pop()?;
        let node = &self.treap.nodes[idx as usize];
        self.push_right(node.left);
        Some((&node.key, &node.value))
    }
}

// Bounded ordered iteration. Default path, not a feature: range scan is the
// reason to reach for an ordered index in the first place.
mod range;
pub use range::{RangeBound, RangeIter};

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each submodule is gated by its own Cargo
// feature flag. See `Cargo.toml` `[features]` and the cookbook page
// for per-feature semantics + p99 impact.
#[cfg(any(
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

// Crate-level unit tests live in colocated files (org convention:
// `<module>_tests.rs` alongside the module), not the top-level `tests/` dir.
#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
