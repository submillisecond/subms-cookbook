//! Clock-sweep block cache. Fixed capacity. Constant-time eviction.
//!
//! Slots form a ring. Each slot has a referenced bit. On insert, if the
//! cache is full, the hand walks the ring: if the slot's referenced bit is
//! set, clear it; if not, evict that slot. Reads set the referenced bit on
//! the hit slot. This is the second-chance variant of LRU - fewer per-op
//! pointer-chasing costs, near-LRU eviction quality.
//!
//! ```
//! use subms_block_cache::BlockCache;
//! let mut c: BlockCache<u32, &'static str> = BlockCache::with_capacity(4);
//! c.put(1, "one");
//! c.put(2, "two");
//! assert_eq!(c.get(&1), Some(&"one"));
//! ```

// Opt-in feature modules. Each is independent of the base cache and
// gated by its own Cargo feature; `cargo add subms-block-cache` alone
// keeps the base zero-dep + std-only shape.
//
// The `clock-sweep` policy from the original feature menu IS the base
// in this recipe, so it has no separate feature gate.
#[cfg(any(
    feature = "arc",
    feature = "tinylfu",
    feature = "weighted",
    feature = "concurrent-shards",
    feature = "metrics",
))]
pub mod features;

#[cfg(feature = "arc")]
pub use features::arc::ArcCache;
#[cfg(feature = "concurrent-shards")]
pub use features::concurrent_shards::ShardedCache;
#[cfg(feature = "metrics")]
pub use features::metrics::{CacheMetrics, MetricsCache};
#[cfg(feature = "tinylfu")]
pub use features::tinylfu::TinyLfuCache;
#[cfg(feature = "weighted")]
pub use features::weighted::WeightedCache;

use std::collections::HashMap;

pub struct BlockCache<K, V> {
    capacity: usize,
    /// Slot storage. None = empty slot.
    slots: Vec<Option<Slot<K, V>>>,
    /// Map key -> slot index.
    index: HashMap<K, usize>,
    /// Clock hand position.
    hand: usize,
}

struct Slot<K, V> {
    key: K,
    value: V,
    referenced: bool,
}

impl<K: std::hash::Hash + Eq + Clone, V> BlockCache<K, V> {
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.max(1);
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(None);
        }
        Self {
            capacity: cap,
            slots,
            index: HashMap::with_capacity(cap),
            hand: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Get a reference to the value for `key`, marking the slot referenced
    /// for the clock sweep.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let idx = *self.index.get(key)?;
        let slot = self.slots[idx].as_mut().expect("indexed slot is populated");
        slot.referenced = true;
        Some(&self.slots[idx].as_ref().unwrap().value)
    }

    /// Insert or update. Returns the evicted (key, value) pair if eviction
    /// happened to make room.
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        if let Some(&idx) = self.index.get(&key) {
            // Update in place.
            let slot = self.slots[idx].as_mut().unwrap();
            slot.value = value;
            slot.referenced = true;
            return None;
        }

        if self.index.len() < self.capacity {
            // Find an empty slot.
            for i in 0..self.capacity {
                if self.slots[i].is_none() {
                    self.slots[i] = Some(Slot {
                        key: key.clone(),
                        value,
                        referenced: true,
                    });
                    self.index.insert(key, i);
                    return None;
                }
            }
            unreachable!("under capacity but no empty slot");
        }

        // Evict via clock sweep.
        loop {
            let idx = self.hand;
            self.hand = (self.hand + 1) % self.capacity;
            let slot = self.slots[idx].as_mut().expect("populated");
            if slot.referenced {
                slot.referenced = false;
                continue;
            }
            // Evict this slot.
            let old = self.slots[idx].take().unwrap();
            self.index.remove(&old.key);
            self.slots[idx] = Some(Slot {
                key: key.clone(),
                value,
                referenced: true,
            });
            self.index.insert(key, idx);
            return Some((old.key, old.value));
        }
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
