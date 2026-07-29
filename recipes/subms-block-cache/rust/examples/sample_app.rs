//! Sample app: a tour of `subms-block-cache`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features arc`) to see the feature
//! sections light up.
//!
//! The scenario throughout is a hot block cache in front of a cold columnar
//! store - a market-data query engine reads fixed-size column blocks (pages)
//! by id, and the cache serves repeat reads without paying the cold fetch.
//!
//! * base              - bounded page cache, clock-sweep eviction
//! * arc               - scan-resistant cache that holds the frequent set
//! * tinylfu           - frequency-gated admission against a one-shot scan
//! * weighted          - byte-budgeted eviction for variable-size pages
//! * concurrent-shards - many query threads reading pages without contending
//! * metrics           - hit-ratio observability on the cache

use subms_block_cache::BlockCache;

fn main() {
    base_page_cache();

    #[cfg(feature = "arc")]
    arc_scan_resistance();

    #[cfg(feature = "tinylfu")]
    tinylfu_admission();

    #[cfg(feature = "weighted")]
    weighted_byte_budget();

    #[cfg(feature = "concurrent-shards")]
    sharded_parallel_readers();

    #[cfg(feature = "metrics")]
    metrics_hit_ratio();
}

/// Stand-in for a cold columnar-store fetch: decompress one page.
fn read_block(id: u64) -> String {
    format!("page:{id}")
}

/// Base API: a bounded cache warms from the store on a miss and serves the hot
/// working set on repeat reads. When full, admitting a new page evicts the
/// coldest slot and hands it back for write-back or drop.
fn base_page_cache() {
    println!("== base: block cache in front of a cold columnar store ==");
    const CAP: usize = 4;
    let mut cache: BlockCache<u64, String> = BlockCache::with_capacity(CAP);

    let hot = [100u64, 101, 102, 103];
    let mut cold_reads = 0;
    for &id in &hot {
        assert!(cache.get(&id).is_none(), "cold on first touch");
        cache.put(id, read_block(id));
        cold_reads += 1;
    }
    println!("  warmed {} pages, {cold_reads} cold reads", cache.len());
    assert_eq!(cache.len(), CAP);

    let mut served = 0;
    for &id in &hot {
        assert!(cache.get(&id).is_some(), "resident page must never miss");
        served += 1;
    }
    println!("  re-read the hot set: {served} served, 0 cold reads");
    assert_eq!(served, CAP);

    let (victim, _) = cache
        .put(200, read_block(200))
        .expect("full cache must evict to admit a new page");
    println!("  admitted page 200, evicted page {victim}");
    assert_eq!(cache.len(), CAP, "capacity is a hard bound");
}

/// `arc` feature: clock-sweep churns the whole cache under a scan. ARC keeps a
/// frequently-read set (its T2 list) alive because a one-shot scan only ever
/// lands in T1 and is evicted to the B1 ghost list without touching T2.
#[cfg(feature = "arc")]
fn arc_scan_resistance() {
    use subms_block_cache::ArcCache;
    println!("\n== arc: scan-resistant page cache ==");
    let mut cache: ArcCache<u64, String> = ArcCache::with_capacity(8);
    for id in 0u64..4 {
        cache.put(id, read_block(id));
        let _ = cache.get(&id); // second touch promotes into the frequent list
    }
    println!("  frequent set in T2: {} pages", cache.t2_len());

    for id in 1000u64..1200 {
        cache.put(id, read_block(id));
    }
    let survivors = (0u64..4).filter(|id| cache.get(id).is_some()).count();
    println!("  hot pages surviving a 200-page scan: {survivors}/4");
    assert_eq!(
        survivors, 4,
        "ARC must hold the frequent set through a scan"
    );
}

/// `tinylfu` feature: a W-TinyLFU admission filter makes a scan page fight the
/// resident it would displace, using a count-min-sketch of access frequency.
/// Low-frequency one-shot pages lose the fight and never enter the cache.
#[cfg(feature = "tinylfu")]
fn tinylfu_admission() {
    use subms_block_cache::TinyLfuCache;
    println!("\n== tinylfu: frequency-gated admission ==");
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
    let rejected = cache.rejections() - rej_before;
    println!(
        "  admissions {}, scan pages rejected {rejected}",
        cache.admissions()
    );
    assert!(
        rejected > 0,
        "admission filter must reject some one-shot scan pages"
    );
}

/// `weighted` feature: pages vary in compressed size, so the budget is bytes,
/// not slots. A `put` evicts until the new page fits; a page larger than the
/// whole budget is handed straight back rather than flushing the resident set.
#[cfg(feature = "weighted")]
fn weighted_byte_budget() {
    use subms_block_cache::WeightedCache;
    println!("\n== weighted: byte-budgeted page cache ==");
    let mut cache: WeightedCache<u64, Vec<u8>> =
        WeightedCache::with_capacity_bytes(4096, |page: &Vec<u8>| page.len());

    let mut evicted_total = 0;
    for id in 0u64..64 {
        let size = 128 + (id as usize % 8) * 128;
        evicted_total += cache.put(id, vec![0u8; size]).len();
    }
    println!(
        "  used {} / 4096 bytes, {} pages, {evicted_total} evicted",
        cache.used_bytes(),
        cache.len()
    );
    assert!(cache.used_bytes() <= 4096, "byte budget is a hard bound");

    let too_big = cache.put(999, vec![0u8; 8192]);
    assert_eq!(too_big.len(), 1, "oversized page is rejected, not admitted");
    assert!(cache.get(&999).is_none());
}

/// `concurrent-shards` feature: the keyspace splits across independent shards
/// by hash, each behind its own lock, so query threads touching different
/// shards never block each other.
#[cfg(feature = "concurrent-shards")]
fn sharded_parallel_readers() {
    use std::sync::Arc;
    use std::thread;
    use subms_block_cache::ShardedCache;
    println!("\n== concurrent-shards: parallel query threads ==");
    let cache: Arc<ShardedCache<u64, u64>> = Arc::new(ShardedCache::with_capacity(1024, 8));
    println!("  {} shards", cache.num_shards());
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
    println!("  survived concurrent load, {} pages resident", cache.len());
    assert!(cache.len() <= 1024, "capacity holds across all shards");
}

/// `metrics` feature: per-instance hit / miss counters and a `hit_ratio()`
/// helper, the number you actually watch to decide whether the cache is
/// earning its footprint in front of the store.
#[cfg(feature = "metrics")]
fn metrics_hit_ratio() {
    use subms_block_cache::MetricsCache;
    println!("\n== metrics: hit-ratio observability ==");
    let mut cache: MetricsCache<u64, String> = MetricsCache::with_capacity(4);
    for id in 0u64..4 {
        cache.put(id, read_block(id));
    }
    let _ = cache.get(&0);
    let _ = cache.get(&1);
    let _ = cache.get(&2);
    let _ = cache.get(&99);
    let m = cache.metrics();
    println!(
        "  hits {}, misses {}, hit ratio {:.2}",
        m.hits(),
        m.misses(),
        m.hit_ratio()
    );
    assert_eq!(m.hits(), 3);
    assert_eq!(m.misses(), 1);
    assert!((m.hit_ratio() - 0.75).abs() < 1e-9);
}
