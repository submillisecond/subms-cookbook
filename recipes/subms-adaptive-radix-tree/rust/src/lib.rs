//! Adaptive Radix Tree (ART) over byte-string keys - Leis et al., 2013.
//!
//! Two ideas carry the structure:
//!
//! * **Adaptive nodes.** Child storage grows with fan-out: `Node4` (up to 4
//!   children, linear scan), `Node16` (up to 16), `Node48` (a 256-entry byte
//!   index into 48 slots), `Node256` (direct 256-way). A node promotes to the
//!   next size when it fills and demotes when `compaction` shrinks it, so a
//!   sparse node never pays for 256 pointers and a dense one never pays a scan.
//! * **Path compression.** A run of single-child bytes collapses into one
//!   node's `prefix`, so a long shared key stem costs one node, not one per
//!   byte. On a diverging insert the node splits at the first mismatch.
//!
//! ```
//! use subms_adaptive_radix_tree::Art;
//! let mut t: Art<i32> = Art::new();
//! t.insert(b"alice", 1);
//! t.insert(b"alicia", 2);   // shares "ali", splits at the 4th byte
//! assert_eq!(t.get(b"alice").copied(), Some(1));
//! assert_eq!(t.get(b"missing"), None);
//! ```
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-adaptive-radix-tree>

pub struct Art<V> {
    root: Node<V>,
    len: usize,
}

pub(crate) struct Node<V> {
    /// Path-compressed bytes shared by every key under this node, matched before
    /// the branching byte selects a child.
    pub(crate) prefix: Vec<u8>,
    /// Value of the key that terminates at this node (after `prefix`), if any.
    pub(crate) value: Option<V>,
    pub(crate) children: Children<V>,
}

/// Which adaptive node layout a `Children` currently uses. Exposed for the
/// `metrics` feature's node-type distribution.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Node4,
    Node16,
    Node48,
    Node256,
}

pub(crate) enum Children<V> {
    Node4 {
        keys: [u8; 4],
        child: [Option<Box<Node<V>>>; 4],
        count: u8,
    },
    Node16 {
        keys: [u8; 16],
        // Boxed so a Node16 does not bloat every `Children` to its size - each
        // node kind should cost roughly its own capacity, ART's memory point.
        child: Box<[Option<Box<Node<V>>>; 16]>,
        count: u8,
    },
    Node48 {
        /// `index[b] == 0` means absent; otherwise the child is `child[index[b] - 1]`.
        index: Box<[u8; 256]>,
        child: Box<[Option<Box<Node<V>>>; 48]>,
        count: u8,
    },
    Node256 {
        child: Box<[Option<Box<Node<V>>>; 256]>,
        count: u16,
    },
}

impl<V> Default for Art<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Art<V> {
    pub fn new() -> Self {
        Self {
            root: Node::inner(Vec::new()),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Insert or replace. Returns the prior value if the key was already present.
    pub fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        let (prior, added) = insert_rec(&mut self.root, key, value);
        if added {
            self.len += 1;
        }
        prior
    }

    pub fn get(&self, key: &[u8]) -> Option<&V> {
        let mut node = &self.root;
        let mut depth = 0usize;
        loop {
            let p = node.prefix.len();
            if key.len() < depth + p || key[depth..depth + p] != node.prefix[..] {
                return None;
            }
            depth += p;
            if depth == key.len() {
                return node.value.as_ref();
            }
            node = node.children.get(key[depth])?;
            depth += 1;
        }
    }

    // Accessors below are used only by the opt-in feature modules (serialize /
    // range-scan / concurrent-reads / metrics / compaction); a feature-free
    // build compiles none of them, so `allow(dead_code)` keeps it warning-free.
    #[allow(dead_code)]
    pub(crate) fn root(&self) -> &Node<V> {
        &self.root
    }

    #[allow(dead_code)]
    pub(crate) fn root_mut(&mut self) -> &mut Node<V> {
        &mut self.root
    }

    #[allow(dead_code)]
    pub(crate) fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    /// Remove the value at `key`, leaving the (now valueless) path in place - run
    /// `compaction` to reclaim it. Returns the removed value if any.
    #[allow(dead_code)]
    pub(crate) fn delete_value(&mut self, key: &[u8]) -> Option<V> {
        let node = walk_mut(&mut self.root, key)?;
        let prior = node.value.take();
        if prior.is_some() {
            self.len -= 1;
        }
        prior
    }
}

impl<V> Node<V> {
    pub(crate) fn inner(prefix: Vec<u8>) -> Self {
        Self {
            prefix,
            value: None,
            children: Children::new(),
        }
    }

    fn leaf(prefix: Vec<u8>, value: V) -> Self {
        Self {
            prefix,
            value: Some(value),
            children: Children::new(),
        }
    }
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i < n && a[i] == b[i] {
        i += 1;
    }
    i
}

/// `key` is the portion of the search key remaining at `node`. Returns
/// `(prior_value, added_new_key)`.
fn insert_rec<V>(node: &mut Node<V>, key: &[u8], value: V) -> (Option<V>, bool) {
    let common = common_prefix_len(&node.prefix, key);
    if common < node.prefix.len() {
        // The node's prefix diverges from the key: split at `common`.
        split_node(node, key, value, common);
        return (None, true);
    }
    // Whole prefix matched - consume it.
    let key = &key[node.prefix.len()..];
    if key.is_empty() {
        let prior = node.value.replace(value);
        let added = prior.is_none();
        return (prior, added);
    }
    let b = key[0];
    let rest = &key[1..];
    if node.children.get(b).is_some() {
        let child = node.children.get_mut(b).unwrap();
        insert_rec(child, rest, value)
    } else {
        node.children
            .insert(b, Box::new(Node::leaf(rest.to_vec(), value)));
        (None, true)
    }
}

/// Split `node` at prefix position `common` (which is `< node.prefix.len()`):
/// a fresh parent takes `prefix[..common]`, the old node drops to a child under
/// byte `prefix[common]` with `prefix[common + 1..]`, and the new key branches
/// beside it (or terminates in the parent).
fn split_node<V>(node: &mut Node<V>, key: &[u8], value: V, common: usize) {
    let mut old = std::mem::replace(node, Node::inner(Vec::new()));
    let old_prefix = std::mem::take(&mut old.prefix);
    let parent_prefix = old_prefix[..common].to_vec();
    let old_edge = old_prefix[common];
    old.prefix = old_prefix[common + 1..].to_vec();

    node.prefix = parent_prefix;
    node.children.insert(old_edge, Box::new(old));

    let krest = &key[common..];
    if krest.is_empty() {
        node.value = Some(value);
    } else {
        node.children
            .insert(krest[0], Box::new(Node::leaf(krest[1..].to_vec(), value)));
    }
}

#[allow(dead_code)] // reached only via delete_value (feature-only)
fn walk_mut<'a, V>(node: &'a mut Node<V>, key: &[u8]) -> Option<&'a mut Node<V>> {
    let mut cur = node;
    let mut depth = 0usize;
    loop {
        let p = cur.prefix.len();
        if key.len() < depth + p || key[depth..depth + p] != cur.prefix[..] {
            return None;
        }
        depth += p;
        if depth == key.len() {
            return Some(cur);
        }
        let b = key[depth];
        cur = cur.children.get_mut(b)?;
        depth += 1;
    }
}

impl<V> Children<V> {
    pub(crate) fn new() -> Self {
        Children::Node4 {
            keys: [0u8; 4],
            child: [const { None }; 4],
            count: 0,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn kind(&self) -> NodeKind {
        match self {
            Children::Node4 { .. } => NodeKind::Node4,
            Children::Node16 { .. } => NodeKind::Node16,
            Children::Node48 { .. } => NodeKind::Node48,
            Children::Node256 { .. } => NodeKind::Node256,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        match self {
            Children::Node4 { count, .. } | Children::Node16 { count, .. } => *count as usize,
            Children::Node48 { count, .. } => *count as usize,
            Children::Node256 { count, .. } => *count as usize,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn get(&self, byte: u8) -> Option<&Node<V>> {
        match self {
            Children::Node4 {
                keys, child, count, ..
            } => {
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        return child[i].as_deref();
                    }
                }
                None
            }
            Children::Node16 {
                keys, child, count, ..
            } => {
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        return child[i].as_deref();
                    }
                }
                None
            }
            Children::Node48 { index, child, .. } => {
                let slot = index[byte as usize];
                if slot == 0 {
                    None
                } else {
                    child[(slot - 1) as usize].as_deref()
                }
            }
            Children::Node256 { child, .. } => child[byte as usize].as_deref(),
        }
    }

    pub(crate) fn get_mut(&mut self, byte: u8) -> Option<&mut Node<V>> {
        match self {
            Children::Node4 {
                keys, child, count, ..
            } => {
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        return child[i].as_deref_mut();
                    }
                }
                None
            }
            Children::Node16 {
                keys, child, count, ..
            } => {
                for i in 0..(*count as usize) {
                    if keys[i] == byte {
                        return child[i].as_deref_mut();
                    }
                }
                None
            }
            Children::Node48 { index, child, .. } => {
                let slot = index[byte as usize];
                if slot == 0 {
                    None
                } else {
                    child[(slot - 1) as usize].as_deref_mut()
                }
            }
            Children::Node256 { child, .. } => child[byte as usize].as_deref_mut(),
        }
    }

    /// Insert a child under `byte` (which must not already be present), growing
    /// to the next node size first if this one is full.
    pub(crate) fn insert(&mut self, byte: u8, node: Box<Node<V>>) {
        if self.is_full() {
            self.grow();
        }
        match self {
            Children::Node4 {
                keys, child, count, ..
            } => {
                let i = *count as usize;
                keys[i] = byte;
                child[i] = Some(node);
                *count += 1;
            }
            Children::Node16 {
                keys, child, count, ..
            } => {
                let i = *count as usize;
                keys[i] = byte;
                child[i] = Some(node);
                *count += 1;
            }
            Children::Node48 {
                index,
                child,
                count,
            } => {
                let i = *count as usize;
                child[i] = Some(node);
                index[byte as usize] = (i + 1) as u8;
                *count += 1;
            }
            Children::Node256 { child, count } => {
                child[byte as usize] = Some(node);
                *count += 1;
            }
        }
    }

    fn is_full(&self) -> bool {
        match self {
            Children::Node4 { count, .. } => *count == 4,
            Children::Node16 { count, .. } => *count == 16,
            Children::Node48 { count, .. } => *count == 48,
            Children::Node256 { .. } => false,
        }
    }

    fn grow(&mut self) {
        match self {
            Children::Node4 {
                keys, child, count, ..
            } => {
                let mut nkeys = [0u8; 16];
                let mut nchild: [Option<Box<Node<V>>>; 16] = [const { None }; 16];
                for i in 0..(*count as usize) {
                    nkeys[i] = keys[i];
                    nchild[i] = child[i].take();
                }
                *self = Children::Node16 {
                    keys: nkeys,
                    child: Box::new(nchild),
                    count: *count,
                };
            }
            Children::Node16 {
                keys, child, count, ..
            } => {
                let mut index = [0u8; 256];
                let mut nchild: [Option<Box<Node<V>>>; 48] = [const { None }; 48];
                for i in 0..(*count as usize) {
                    nchild[i] = child[i].take();
                    index[keys[i] as usize] = (i + 1) as u8;
                }
                *self = Children::Node48 {
                    index: Box::new(index),
                    child: Box::new(nchild),
                    count: *count,
                };
            }
            Children::Node48 {
                index,
                child,
                count,
            } => {
                let mut nchild: [Option<Box<Node<V>>>; 256] = [const { None }; 256];
                for b in 0..256usize {
                    let slot = index[b];
                    if slot != 0 {
                        nchild[b] = child[(slot - 1) as usize].take();
                    }
                }
                *self = Children::Node256 {
                    child: Box::new(nchild),
                    count: *count as u16,
                };
            }
            Children::Node256 { .. } => {}
        }
    }

    /// `(byte, child)` pairs in ascending byte order. Used by the feature modules
    /// that reconstruct keys or serialize the tree.
    #[allow(dead_code)]
    pub(crate) fn sorted_pairs(&self) -> Vec<(u8, &Node<V>)> {
        let mut out: Vec<(u8, &Node<V>)> = match self {
            Children::Node4 {
                keys, child, count, ..
            } => (0..(*count as usize))
                .filter_map(|i| child[i].as_deref().map(|c| (keys[i], c)))
                .collect(),
            Children::Node16 {
                keys, child, count, ..
            } => (0..(*count as usize))
                .filter_map(|i| child[i].as_deref().map(|c| (keys[i], c)))
                .collect(),
            Children::Node48 { index, child, .. } => (0..256usize)
                .filter_map(|b| {
                    let slot = index[b];
                    if slot == 0 {
                        None
                    } else {
                        child[(slot - 1) as usize].as_deref().map(|c| (b as u8, c))
                    }
                })
                .collect(),
            Children::Node256 { child, .. } => (0..256usize)
                .filter_map(|b| child[b].as_deref().map(|c| (b as u8, c)))
                .collect(),
        };
        out.sort_by_key(|(b, _)| *b);
        out
    }

    /// Mutable `(byte, child)` pairs, unordered. Used by `compaction` to walk and
    /// rewrite the subtree.
    #[allow(dead_code)]
    pub(crate) fn each_child_mut(&mut self, mut f: impl FnMut(&mut Node<V>)) {
        match self {
            Children::Node4 { child, count, .. } => {
                for slot in child.iter_mut().take(*count as usize) {
                    if let Some(c) = slot.as_deref_mut() {
                        f(c);
                    }
                }
            }
            Children::Node16 { child, count, .. } => {
                for slot in child.iter_mut().take(*count as usize) {
                    if let Some(c) = slot.as_deref_mut() {
                        f(c);
                    }
                }
            }
            Children::Node48 { child, .. } => {
                for slot in child.iter_mut() {
                    if let Some(c) = slot.as_deref_mut() {
                        f(c);
                    }
                }
            }
            Children::Node256 { child, .. } => {
                for slot in child.iter_mut() {
                    if let Some(c) = slot.as_deref_mut() {
                        f(c);
                    }
                }
            }
        }
    }

    /// Remove the child under `byte`, if present, returning it. Does not demote
    /// the node size - `compaction` decides when to shrink.
    #[allow(dead_code)]
    pub(crate) fn remove(&mut self, byte: u8) -> Option<Box<Node<V>>> {
        match self {
            Children::Node4 {
                keys, child, count, ..
            } => {
                let n = *count as usize;
                for i in 0..n {
                    if keys[i] == byte {
                        let removed = child[i].take();
                        keys[i] = keys[n - 1];
                        child[i] = child[n - 1].take();
                        keys[n - 1] = 0;
                        *count -= 1;
                        return removed;
                    }
                }
                None
            }
            Children::Node16 {
                keys, child, count, ..
            } => {
                let n = *count as usize;
                for i in 0..n {
                    if keys[i] == byte {
                        let removed = child[i].take();
                        // Compact the arrays: move the last entry into the hole.
                        keys[i] = keys[n - 1];
                        child[i] = child[n - 1].take();
                        keys[n - 1] = 0;
                        *count -= 1;
                        return removed;
                    }
                }
                None
            }
            Children::Node48 {
                index,
                child,
                count,
            } => {
                let slot = index[byte as usize];
                if slot == 0 {
                    return None;
                }
                let removed = child[(slot - 1) as usize].take();
                index[byte as usize] = 0;
                *count -= 1;
                removed
            }
            Children::Node256 { child, count } => {
                let removed = child[byte as usize].take();
                if removed.is_some() {
                    *count -= 1;
                }
                removed
            }
        }
    }

    /// Drain every `(byte, child)` out, leaving an empty `Node4`. Used by
    /// `compaction` to rebuild a node at a smaller size (re-inserting auto-grows
    /// to the minimal layout for the occupancy).
    #[allow(dead_code)]
    pub(crate) fn take_all(&mut self) -> Vec<(u8, Box<Node<V>>)> {
        let taken = std::mem::replace(self, Children::new());
        match taken {
            Children::Node4 {
                keys,
                mut child,
                count,
            } => (0..count as usize)
                .filter_map(|i| child[i].take().map(|c| (keys[i], c)))
                .collect(),
            Children::Node16 {
                keys,
                mut child,
                count,
            } => (0..count as usize)
                .filter_map(|i| child[i].take().map(|c| (keys[i], c)))
                .collect(),
            Children::Node48 {
                index, mut child, ..
            } => (0..256usize)
                .filter_map(|b| {
                    let slot = index[b];
                    if slot == 0 {
                        None
                    } else {
                        child[(slot - 1) as usize].take().map(|c| (b as u8, c))
                    }
                })
                .collect(),
            Children::Node256 { mut child, .. } => (0..256usize)
                .filter_map(|b| child[b].take().map(|c| (b as u8, c)))
                .collect(),
        }
    }

    /// Insert-or-get the child under `byte`, creating an empty inner node if
    /// absent. Used by `serialize` while rebuilding a tree from bytes.
    #[allow(dead_code)]
    pub(crate) fn get_or_insert_for_load(&mut self, byte: u8) -> &mut Node<V> {
        if self.get(byte).is_none() {
            self.insert(byte, Box::new(Node::inner(Vec::new())));
        }
        self.get_mut(byte).unwrap()
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each submodule is gated by its own Cargo feature;
// `cargo add subms-adaptive-radix-tree` alone keeps the base zero-dep + std-only.
#[cfg(any(
    feature = "serialize",
    feature = "range-scan",
    feature = "concurrent-reads",
    feature = "metrics",
    feature = "compaction",
))]
pub mod features;

#[cfg(feature = "compaction")]
pub use features::compaction::{compact, delete};
#[cfg(feature = "concurrent-reads")]
pub use features::concurrent_reads::ArtSnapshot;
#[cfg(feature = "metrics")]
pub use features::metrics::{ArtMetrics, MeasuredArt, NodeTypeCounts};
#[cfg(feature = "range-scan")]
pub use features::range_scan::{Bound, range};
#[cfg(feature = "serialize")]
pub use features::serialize::{ArtCodec, parse, write_to};

#[cfg(test)]
#[path = "art_tests.rs"]
mod art_tests;
#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
