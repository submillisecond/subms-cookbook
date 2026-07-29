//! Read-path block cache wiring.
//!
//! The read path consults a [`BlockCache`] before reading from disk. This
//! module ships:
//! - [`BlockKey`] - the (sstable_id, block_offset) cache key.
//! - [`Block`] - an owned byte buffer; `Arc<[u8]>` so the cache can hand the
//!   same payload to many concurrent readers without copying.
//! - [`BlockCache`] - the trait the read path calls.
//! - [`LruBlockCache`] - a working LRU implementation suitable for the LSM
//!   tree's modest cardinality + single-thread base. A real production
//!   replacement (TinyLFU, segmented LRU) lives in the sibling recipe
//!   `subms-block-cache`; this module is the wiring contract, not the
//!   policy ceiling.
//!
//! The base [`crate::LsmTree`] does NOT depend on this module. It's a
//! standalone trait + reference impl that an integrator wires in via their
//! own custom read path - mirrors the bloom-filter recipe's relationship.

use std::collections::HashMap;
use std::sync::Arc;

/// Cache lookup key: (sstable id, block byte offset within the file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockKey {
    pub sstable_id: u64,
    pub block_offset: u64,
}

impl BlockKey {
    pub fn new(sstable_id: u64, block_offset: u64) -> Self {
        Self {
            sstable_id,
            block_offset,
        }
    }
}

/// Cached block payload. `Arc<[u8]>` so multiple readers see the same bytes
/// without paying for a copy on lookup.
pub type Block = Arc<[u8]>;

/// The trait the read path calls. Implementations decide replacement policy,
/// concurrency, and whether to admit insertions.
pub trait BlockCache: Send + Sync {
    /// Returns the cached payload if present.
    fn get(&self, key: &BlockKey) -> Option<Block>;

    /// Insert a block. May evict to honour capacity bounds.
    fn put(&self, key: BlockKey, block: Block);

    /// Current number of cached entries.
    fn len(&self) -> usize;

    /// True if no entries are cached.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every entry. Used by tests + manifest swaps.
    fn clear(&self);
}

/// Bounded LRU. Uses a doubly-linked list of node indices plus a hashmap for
/// O(1) lookup. The list is implemented over a `Vec<Node>` to avoid the
/// allocator overhead of `Box<Node>` per insert.
///
/// Single-threaded under the hood; the trait advertises `Sync` via an
/// internal mutex so the cache can be shared across reader threads.
pub struct LruBlockCache {
    inner: std::sync::Mutex<LruInner>,
}

struct LruInner {
    capacity: usize,
    map: HashMap<BlockKey, usize>,
    nodes: Vec<Node>,
    head: Option<usize>,
    tail: Option<usize>,
    free: Vec<usize>,
    hits: u64,
    misses: u64,
}

struct Node {
    key: BlockKey,
    block: Block,
    prev: Option<usize>,
    next: Option<usize>,
}

impl LruBlockCache {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            inner: std::sync::Mutex::new(LruInner {
                capacity: cap,
                map: HashMap::with_capacity(cap),
                nodes: Vec::with_capacity(cap),
                head: None,
                tail: None,
                free: Vec::new(),
                hits: 0,
                misses: 0,
            }),
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.lock().unwrap().capacity
    }

    pub fn hits(&self) -> u64 {
        self.inner.lock().unwrap().hits
    }

    pub fn misses(&self) -> u64 {
        self.inner.lock().unwrap().misses
    }
}

impl BlockCache for LruBlockCache {
    fn get(&self, key: &BlockKey) -> Option<Block> {
        let mut g = self.inner.lock().unwrap();
        match g.map.get(key).copied() {
            Some(idx) => {
                let block = g.nodes[idx].block.clone();
                g.move_to_front(idx);
                g.hits += 1;
                Some(block)
            }
            None => {
                g.misses += 1;
                None
            }
        }
    }

    fn put(&self, key: BlockKey, block: Block) {
        let mut g = self.inner.lock().unwrap();
        // Already cached: refresh the payload, bump to front, done.
        if let Some(&idx) = g.map.get(&key) {
            g.nodes[idx].block = block;
            g.move_to_front(idx);
            return;
        }
        // Evict if at capacity.
        if g.map.len() >= g.capacity {
            if let Some(tail_idx) = g.tail {
                let tail_key = g.nodes[tail_idx].key;
                g.detach(tail_idx);
                g.map.remove(&tail_key);
                g.free.push(tail_idx);
            }
        }
        // Allocate.
        let idx = if let Some(slot) = g.free.pop() {
            g.nodes[slot] = Node {
                key,
                block,
                prev: None,
                next: None,
            };
            slot
        } else {
            g.nodes.push(Node {
                key,
                block,
                prev: None,
                next: None,
            });
            g.nodes.len() - 1
        };
        g.map.insert(key, idx);
        g.push_front(idx);
    }

    fn len(&self) -> usize {
        self.inner.lock().unwrap().map.len()
    }

    fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.map.clear();
        g.nodes.clear();
        g.free.clear();
        g.head = None;
        g.tail = None;
    }
}

impl LruInner {
    fn push_front(&mut self, idx: usize) {
        self.nodes[idx].prev = None;
        self.nodes[idx].next = self.head;
        if let Some(h) = self.head {
            self.nodes[h].prev = Some(idx);
        }
        self.head = Some(idx);
        if self.tail.is_none() {
            self.tail = Some(idx);
        }
    }

    fn detach(&mut self, idx: usize) {
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;
        if let Some(p) = prev {
            self.nodes[p].next = next;
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            self.nodes[n].prev = prev;
        } else {
            self.tail = prev;
        }
        self.nodes[idx].prev = None;
        self.nodes[idx].next = None;
    }

    fn move_to_front(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return;
        }
        self.detach(idx);
        self.push_front(idx);
    }
}

#[cfg(test)]
#[path = "block_cache_integration_tests.rs"]
mod tests;
