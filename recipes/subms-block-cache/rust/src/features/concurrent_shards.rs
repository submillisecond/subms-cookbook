//! Sharded block cache: split keyspace into N independent shards by
//! `hash(key) % N`. Each shard wraps the base `BlockCache` behind its
//! own `Mutex`, so concurrent readers/writers on different shards
//! don't contend.
//!
//! Per-shard contention counter (`try_lock` failures) is exposed via
//! `contention_events()` for the `metrics` feature integration. The
//! ShardedCache itself only tracks the counter when a put/get path
//! actually backs off; it does NOT change correctness behaviour - if
//! `try_lock` would have failed, we fall through to the blocking lock
//! and still complete the op.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::BlockCache;

pub struct ShardedCache<K, V> {
    shards: Vec<Mutex<BlockCache<K, V>>>,
    contention: AtomicU64,
}

impl<K, V> ShardedCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a sharded cache with `total_capacity` distributed across
    /// `num_shards` shards (rounded up). Each shard gets at least 1 slot.
    pub fn with_capacity(total_capacity: usize, num_shards: usize) -> Self {
        let shards_n = num_shards.max(1).next_power_of_two();
        let per_shard = total_capacity.div_ceil(shards_n).max(1);
        let mut shards = Vec::with_capacity(shards_n);
        for _ in 0..shards_n {
            shards.push(Mutex::new(BlockCache::with_capacity(per_shard)));
        }
        Self {
            shards,
            contention: AtomicU64::new(0),
        }
    }

    pub fn num_shards(&self) -> usize {
        self.shards.len()
    }
    pub fn contention_events(&self) -> u64 {
        self.contention.load(Ordering::Relaxed)
    }

    /// Aggregate length across shards. Snapshot; under concurrent
    /// access the returned number may be slightly stale.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|m| m.lock().unwrap().len()).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn shard_index(&self, key: &K) -> usize {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        (h.finish() as usize) & (self.shards.len() - 1)
    }

    /// Get a cloned value for `key`. Cloning matters because returning
    /// a borrow would extend the MutexGuard across the call site.
    pub fn get(&self, key: &K) -> Option<V> {
        let idx = self.shard_index(key);
        let guard = self.lock_with_contention(idx);
        let mut g = guard;
        g.get(key).cloned()
    }

    /// Insert or update. Returns the evicted entry if eviction occurred.
    pub fn put(&self, key: K, value: V) -> Option<(K, V)> {
        let idx = self.shard_index(&key);
        let mut g = self.lock_with_contention(idx);
        g.put(key, value)
    }

    /// Invalidate `key` in its own shard. Only that shard is locked.
    pub fn remove(&self, key: &K) -> Option<V> {
        let idx = self.shard_index(key);
        let mut g = self.lock_with_contention(idx);
        g.remove(key)
    }

    /// Drop every entry. Shards are cleared one at a time, so a concurrent
    /// writer can land in an already-cleared shard - this is a bulk
    /// invalidation, not a global barrier.
    pub fn clear(&self) {
        for shard in &self.shards {
            shard.lock().unwrap().clear();
        }
    }

    fn lock_with_contention(&self, idx: usize) -> std::sync::MutexGuard<'_, BlockCache<K, V>> {
        match self.shards[idx].try_lock() {
            Ok(g) => g,
            Err(_) => {
                self.contention.fetch_add(1, Ordering::Relaxed);
                self.shards[idx].lock().unwrap()
            }
        }
    }
}

#[cfg(test)]
#[path = "concurrent_shards_tests.rs"]
mod tests;
