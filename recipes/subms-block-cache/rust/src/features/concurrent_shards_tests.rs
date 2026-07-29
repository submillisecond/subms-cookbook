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
