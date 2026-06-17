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
mod tests {
    use super::*;

    fn block(bytes: &[u8]) -> Block {
        Arc::from(bytes.to_vec().into_boxed_slice())
    }

    #[test]
    fn miss_then_hit() {
        let c = LruBlockCache::new(4);
        let k = BlockKey::new(1, 0);
        assert!(c.get(&k).is_none());
        c.put(k, block(b"payload"));
        let got = c.get(&k).unwrap();
        assert_eq!(&*got, b"payload");
        assert_eq!(c.hits(), 1);
        assert_eq!(c.misses(), 1);
    }

    #[test]
    fn lru_evicts_coldest() {
        let c = LruBlockCache::new(2);
        let a = BlockKey::new(1, 0);
        let b = BlockKey::new(2, 0);
        let z = BlockKey::new(3, 0);
        c.put(a, block(b"A"));
        c.put(b, block(b"B"));
        // Touch a so b is now the LRU.
        c.get(&a);
        c.put(z, block(b"Z"));
        assert!(c.get(&a).is_some(), "recently-used 'a' stays");
        assert!(c.get(&b).is_none(), "stale 'b' evicted");
        assert!(c.get(&z).is_some(), "newest 'z' kept");
    }

    #[test]
    fn put_of_existing_key_refreshes_value() {
        let c = LruBlockCache::new(2);
        let k = BlockKey::new(1, 0);
        c.put(k, block(b"old"));
        c.put(k, block(b"new"));
        assert_eq!(&*c.get(&k).unwrap(), b"new");
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn clear_drops_everything() {
        let c = LruBlockCache::new(4);
        c.put(BlockKey::new(1, 0), block(b"x"));
        c.put(BlockKey::new(2, 0), block(b"y"));
        assert_eq!(c.len(), 2);
        c.clear();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
    }

    #[test]
    fn capacity_floor_is_one() {
        let c = LruBlockCache::new(0);
        assert_eq!(c.capacity(), 1);
        c.put(BlockKey::new(1, 0), block(b"a"));
        c.put(BlockKey::new(2, 0), block(b"b"));
        assert_eq!(c.len(), 1, "cap-1 cache holds the latest only");
        assert!(c.get(&BlockKey::new(2, 0)).is_some());
        assert!(c.get(&BlockKey::new(1, 0)).is_none());
    }

    #[test]
    fn block_arc_clone_is_shared() {
        let c = LruBlockCache::new(2);
        let k = BlockKey::new(1, 100);
        c.put(k, block(b"shared"));
        let h1 = c.get(&k).unwrap();
        let h2 = c.get(&k).unwrap();
        assert!(
            Arc::ptr_eq(&h1, &h2),
            "cache hits hand out Arc clones, not copies"
        );
    }

    #[test]
    fn hits_and_misses_are_counted() {
        let c = LruBlockCache::new(4);
        let k = BlockKey::new(1, 0);
        c.put(k, block(b"v"));
        c.get(&k);
        c.get(&k);
        c.get(&BlockKey::new(99, 0));
        assert_eq!(c.hits(), 2);
        assert_eq!(c.misses(), 1);
    }

    #[test]
    fn distinct_keys_share_cache_when_under_capacity() {
        let c = LruBlockCache::new(4);
        for i in 0..4 {
            c.put(BlockKey::new(i, 0), block(&[i as u8]));
        }
        for i in 0..4 {
            assert!(c.get(&BlockKey::new(i, 0)).is_some());
        }
        assert_eq!(c.len(), 4);
    }
}
