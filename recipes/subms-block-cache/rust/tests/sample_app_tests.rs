//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base page cache warms + serves + bounds, and each opt-in feature holds
//! the property its section leans on.

use subms_block_cache::BlockCache;

fn read_block(id: u64) -> String {
    format!("page:{id}")
}

#[test]
fn base_page_cache_scenario() {
    let mut cache: BlockCache<u64, String> = BlockCache::with_capacity(4);
    for id in 100u64..104 {
        assert!(cache.get(&id).is_none(), "cold on first touch");
        cache.put(id, read_block(id));
    }
    assert_eq!(cache.len(), 4);

    for id in 100u64..104 {
        assert!(cache.get(&id).is_some(), "a resident page must never miss");
    }

    let evicted = cache.put(200, read_block(200));
    assert!(evicted.is_some(), "a full cache evicts to admit a new page");
    assert_eq!(cache.len(), 4, "capacity is a hard bound");
}

#[cfg(feature = "arc")]
#[test]
fn arc_holds_frequent_set_through_scan() {
    use subms_block_cache::ArcCache;
    let mut cache: ArcCache<u64, String> = ArcCache::with_capacity(8);
    for id in 0u64..4 {
        cache.put(id, read_block(id));
        let _ = cache.get(&id);
    }
    assert_eq!(cache.t2_len(), 4, "second touch lifts pages into T2");
    for id in 1000u64..1200 {
        cache.put(id, read_block(id));
    }
    let survivors = (0u64..4).filter(|id| cache.get(id).is_some()).count();
    assert_eq!(survivors, 4, "the scan must not evict the frequent set");
}

#[cfg(feature = "tinylfu")]
#[test]
fn tinylfu_rejects_one_shot_scan_pages() {
    use subms_block_cache::TinyLfuCache;
    let mut cache: TinyLfuCache<u64, String> = TinyLfuCache::with_capacity(64);
    for _ in 0..50 {
        for id in 0u64..16 {
            let _ = cache.get(&id);
        }
    }
    for id in 0u64..16 {
        cache.put(id, read_block(id));
    }
    for _ in 0..50 {
        for id in 0u64..16 {
            let _ = cache.get(&id);
        }
    }
    let rej_before = cache.rejections();
    for id in 1000u64..3000 {
        cache.put(id, read_block(id));
    }
    assert!(
        cache.rejections() > rej_before,
        "admission filter must reject some one-shot scan pages"
    );
}

#[cfg(feature = "weighted")]
#[test]
fn weighted_bounds_bytes_and_rejects_oversized() {
    use subms_block_cache::WeightedCache;
    let mut cache: WeightedCache<u64, Vec<u8>> =
        WeightedCache::with_capacity_bytes(4096, |page: &Vec<u8>| page.len());
    for id in 0u64..64 {
        let size = 128 + (id as usize % 8) * 128;
        cache.put(id, vec![0u8; size]);
    }
    assert!(cache.used_bytes() <= 4096, "byte budget is a hard bound");

    let too_big = cache.put(999, vec![0u8; 8192]);
    assert_eq!(too_big.len(), 1, "oversized page is rejected");
    assert!(cache.get(&999).is_none());
}

#[cfg(feature = "concurrent-shards")]
#[test]
fn sharded_survives_concurrent_load() {
    use std::sync::Arc;
    use std::thread;
    use subms_block_cache::ShardedCache;
    let cache: Arc<ShardedCache<u64, u64>> = Arc::new(ShardedCache::with_capacity(1024, 8));
    assert_eq!(cache.num_shards(), 8);
    for id in 0u64..256 {
        cache.put(id, id);
    }
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for id in 0u64..4000 {
                let _ = c.get(&(id % 256));
            }
        }));
    }
    for t in 0u64..2 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0u64..2000 {
                c.put(t * 10_000 + i, i);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert!(cache.len() <= 1024, "capacity holds across all shards");
}

#[cfg(feature = "metrics")]
#[test]
fn metrics_reports_hit_ratio() {
    use subms_block_cache::MetricsCache;
    let mut cache: MetricsCache<u64, String> = MetricsCache::with_capacity(4);
    for id in 0u64..4 {
        cache.put(id, read_block(id));
    }
    let _ = cache.get(&0);
    let _ = cache.get(&1);
    let _ = cache.get(&2);
    let _ = cache.get(&99);
    assert_eq!(cache.metrics().hits(), 3);
    assert_eq!(cache.metrics().misses(), 1);
    assert!((cache.metrics().hit_ratio() - 0.75).abs() < 1e-9);
}
