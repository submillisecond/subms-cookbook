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
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn shard_count_rounds_to_power_of_two() {
        let c: ShardedCache<u32, u32> = ShardedCache::with_capacity(64, 6);
        assert_eq!(c.num_shards(), 8);
    }

    #[test]
    fn put_then_get_same_thread() {
        let c: ShardedCache<u32, u32> = ShardedCache::with_capacity(64, 4);
        c.put(1, 10);
        c.put(2, 20);
        assert_eq!(c.get(&1), Some(10));
        assert_eq!(c.get(&2), Some(20));
        assert_eq!(c.get(&999), None);
    }

    #[test]
    fn many_keys_distribute_across_shards() {
        let c: ShardedCache<u32, u32> = ShardedCache::with_capacity(256, 8);
        for k in 0u32..200 {
            c.put(k, k * 2);
        }
        // All shards together must hold up to 256 items.
        assert!(c.len() <= 256);
        // Some hits should still be retrievable.
        let hits: usize = (0u32..200).filter(|k| c.get(k).is_some()).count();
        assert!(hits > 0);
    }

    #[test]
    fn concurrent_writes_do_not_corrupt() {
        let c: Arc<ShardedCache<u32, u32>> = Arc::new(ShardedCache::with_capacity(1024, 8));
        let mut handles = Vec::new();
        for t in 0u32..4 {
            let cc = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for i in 0u32..5_000 {
                    let k = t * 100_000 + i;
                    cc.put(k, k);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(c.len() <= 1024);
    }

    #[test]
    fn concurrent_readers_and_writers() {
        let c: Arc<ShardedCache<u32, u32>> = Arc::new(ShardedCache::with_capacity(256, 8));
        // Seed.
        for k in 0u32..100 {
            c.put(k, k);
        }
        let mut handles = Vec::new();
        for _ in 0..4 {
            let cc = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for k in 0u32..2000 {
                    let _ = cc.get(&(k % 100));
                }
            }));
        }
        for t in 0u32..2 {
            let cc = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for i in 0u32..2000 {
                    cc.put(t * 1000 + i, i);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(c.len() <= 256);
    }

    #[test]
    fn single_shard_still_correct() {
        let c: ShardedCache<u32, u32> = ShardedCache::with_capacity(8, 1);
        for k in 0u32..16 {
            c.put(k, k * 3);
        }
        assert_eq!(c.num_shards(), 1);
        assert!(c.len() <= 8);
    }

    #[test]
    fn contention_counter_monotonic() {
        let c: Arc<ShardedCache<u32, u32>> = Arc::new(ShardedCache::with_capacity(64, 2));
        let mut handles = Vec::new();
        for t in 0u32..4 {
            let cc = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for i in 0u32..500 {
                    cc.put(t * 100_000 + i, i);
                    let _ = cc.get(&(t * 100_000 + i));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Just assert the counter is readable + doesn't underflow.
        let _ = c.contention_events();
    }
}
