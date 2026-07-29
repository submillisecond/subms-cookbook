//! Sample app: a tour of `subms-lsm-tree`, base API first, then each opt-in
//! feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features wal`) to light up the
//! feature sections.
//!
//! The framing throughout is an embedded order journal: a per-symbol store of
//! fills keyed by order id, the kind of write-heavy append-shaped state an LSM
//! tree is built for.
//!
//! * base                    - the order journal: put/get, a bloom miss, a cancel-tombstone, a range scan
//! * wal                     - durable append-before-ack; replay recovers an un-flushed memtable
//! * tiered-compaction       - size-tiered merge for a write-heavy ingest tier
//! * leveled-compaction      - leveled merge for a read-latency-SLA serving tier
//! * snapshot                - a point-in-time read view for an end-of-day report
//! * lz4                     - fast block compression for the hot tier
//! * zstd                    - higher-ratio block compression for the cold tier
//! * block-cache-integration - a read-side LRU in front of block IO

use std::env;
use std::io;
use std::path::PathBuf;

use subms_lsm_tree::LsmTree;

fn main() -> io::Result<()> {
    base_order_journal()?;

    #[cfg(feature = "wal")]
    wal_durable_log()?;

    #[cfg(feature = "tiered-compaction")]
    tiered_ingest_tier();

    #[cfg(feature = "leveled-compaction")]
    leveled_serving_tier();

    #[cfg(feature = "snapshot")]
    snapshot_end_of_day_report();

    #[cfg(feature = "lz4")]
    lz4_hot_tier();

    #[cfg(feature = "zstd")]
    zstd_cold_tier();

    #[cfg(feature = "block-cache-integration")]
    block_cache_read_path();

    Ok(())
}

/// A fresh, process-unique data dir under the temp root so repeated runs never
/// read a previous run's SSTables.
fn scratch_dir(label: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lsm-sample-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Base API: a journal of order fills. A small flush threshold rolls a few
/// SSTables so the read path actually walks more than the memtable. Shows the
/// four moves that define the store - a hit, a bloom-accelerated miss on an id
/// that was never written, a cancel that lands as a tombstone, and a sorted
/// range scan over the live book.
fn base_order_journal() -> io::Result<()> {
    println!("== base: embedded order journal ==");
    let dir = scratch_dir("base");
    // 256-byte threshold so a handful of fills spill across a couple of SSTables.
    let mut journal = LsmTree::open(&dir, 256)?;

    journal.put("ORD-0001", b"AAPL,100@150.10")?;
    journal.put("ORD-0002", b"MSFT,50@320.55")?;
    journal.put("ORD-0003", b"GOOG,25@140.20")?;
    journal.flush()?; // roll SSTable_0

    journal.put("ORD-0001", b"AAPL,100@150.42")?; // amended fill shadows the old one
    journal.put("ORD-0004", b"NVDA,10@900.00")?;
    journal.delete("ORD-0002")?; // cancel: a tombstone
    journal.flush()?; // roll SSTable_1

    let filled = journal.get("ORD-0001")?.expect("ORD-0001 is live");
    println!("  ORD-0001 -> {}", String::from_utf8_lossy(&filled));
    assert_eq!(filled, b"AAPL,100@150.42", "newest write wins");

    // A cancelled order reads as absent - the tombstone shadows the older fill.
    assert!(
        journal.get("ORD-0002")?.is_none(),
        "cancelled order is absent"
    );

    // An id that was never written: the per-SSTable bloom answers "no" in a few
    // hash probes, so the miss never scans a single record.
    assert!(
        journal.get("ORD-9999")?.is_none(),
        "unknown id: bloom-accelerated miss"
    );

    let book = journal.range(Some("ORD-0001"), Some("ORD-0005"))?;
    let live_ids: Vec<&str> = book.iter().map(|(k, _)| k.as_str()).collect();
    println!(
        "  live book {live_ids:?} across {} sstables",
        journal.sstable_count()
    );
    assert_eq!(
        live_ids,
        ["ORD-0001", "ORD-0003", "ORD-0004"],
        "sorted, tombstone dropped"
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// `wal` feature: the base tree loses an un-flushed memtable on a crash. The
/// write-ahead log appends every mutation before the write is acked, so a
/// replay rebuilds the surviving records into a fresh memtable on restart. A
/// torn or bad-CRC tail is dropped without poisoning the recovered prefix.
#[cfg(feature = "wal")]
fn wal_durable_log() -> io::Result<()> {
    use subms_lsm_tree::WriteAheadLog;
    println!("\n== wal: durable append-before-ack ==");
    let dir = scratch_dir("wal");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("journal.wal");

    {
        let mut wal = WriteAheadLog::open(&path)?;
        wal.log_put("ORD-0100", b"AAPL,100@150.10")?;
        wal.log_put("ORD-0101", b"MSFT,50@320.55")?;
        wal.log_delete("ORD-0100")?; // cancel, logged too
        wal.sync()?; // force durability, then "crash" (drop the handle)
    }

    let recovered = WriteAheadLog::replay(&path)?;
    println!("  replayed {} records after crash", recovered.len());
    assert_eq!(recovered.len(), 3, "every acked write survives");
    assert!(
        recovered[2].value.is_none(),
        "the cancel replays as a tombstone"
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// `tiered-compaction` feature: a write-heavy ingest tier keeps flushing
/// similar-sized runs. Size-tiered compaction merges N runs at a level into
/// one larger run at the next level, keeping write amplification low at the
/// cost of read/space amplification. The planner is pure logic; the caller
/// owns the actual rewrite.
#[cfg(feature = "tiered-compaction")]
fn tiered_ingest_tier() {
    use subms_lsm_tree::{TieredCompactionPlanner, TieredManifest, TieredRun};
    println!("\n== tiered-compaction: write-heavy ingest tier ==");
    let mut manifest = TieredManifest::new();
    for i in 0..4 {
        let entries = vec![(format!("ORD-{i:04}"), Some(b"fill".to_vec()))];
        manifest.push(0, TieredRun::new(i, entries)); // newest last
    }

    let planner = TieredCompactionPlanner::new(4);
    let level = planner
        .pick_level(&manifest)
        .expect("level 0 is full at 4 runs");
    planner.merge(&mut manifest, level, 100);
    println!(
        "  merged 4 L0 runs -> {} run at L1",
        manifest.level_run_count(1)
    );
    assert_eq!(manifest.level_run_count(0), 0, "L0 drained");
    assert_eq!(manifest.level_run_count(1), 1, "one merged run promoted");
    assert!(
        planner.pick_level(&manifest).is_none(),
        "a single run does not re-trigger"
    );
}

/// `leveled-compaction` feature: a serving tier whose contract is a stable read
/// p99. Leveled compaction keeps each level beyond L0 key-disjoint, so a point
/// read probes at most one run per level - read amplification is bounded. The
/// price is higher write amplification.
#[cfg(feature = "leveled-compaction")]
fn leveled_serving_tier() {
    use subms_lsm_tree::features::leveled_compaction::level_is_non_overlapping;
    use subms_lsm_tree::{LeveledCompactionPlanner, LeveledManifest, LeveledRun};
    println!("\n== leveled-compaction: read-latency-SLA serving tier ==");
    let mut manifest = LeveledManifest::new();
    // Two overlapping L0 runs plus an older L1 run they overlap.
    manifest.push(
        0,
        LeveledRun::new(
            1,
            vec![
                ("AAPL".to_string(), Some(b"150.10".to_vec())),
                ("MSFT".to_string(), Some(b"320.55".to_vec())),
            ],
        ),
    );
    manifest.push(
        0,
        LeveledRun::new(2, vec![("GOOG".to_string(), Some(b"140.20".to_vec()))]),
    );
    manifest.push(
        1,
        LeveledRun::new(
            3,
            vec![
                ("AAPL".to_string(), Some(b"149.00".to_vec())), // stale, will be shadowed
                ("NVDA".to_string(), Some(b"900.00".to_vec())),
            ],
        ),
    );

    let planner = LeveledCompactionPlanner::new(1_000_000, 10, 2);
    let from = planner
        .pick_level(&manifest)
        .expect("L0 over its 2-run limit");
    planner.compact(&mut manifest, from, 100);
    println!(
        "  compacted L0 -> L1: {} run(s) at L1",
        manifest.level_run_count(1)
    );
    assert_eq!(manifest.level_run_count(0), 0, "L0 drained into L1");
    assert!(
        level_is_non_overlapping(&manifest, 1),
        "L1 is key-disjoint after compaction"
    );
}

/// `snapshot` feature: an end-of-day report scans a consistent view while the
/// ingest thread keeps flushing. `snapshot()` pins the manifest behind an
/// `Arc`; publishing a new manifest afterwards does not perturb the held view.
#[cfg(feature = "snapshot")]
fn snapshot_end_of_day_report() {
    use subms_lsm_tree::{SnapshotManager, SnapshotManifest};
    println!("\n== snapshot: point-in-time end-of-day report ==");
    let manager = SnapshotManager::new();
    manager.publish(SnapshotManifest::new(vec![1, 2, 3]));

    let report_view = manager.snapshot(); // the report starts scanning here
    manager.publish(SnapshotManifest::new(vec![1, 2, 3, 4, 5])); // ingest keeps flushing

    println!(
        "  report sees {:?}, live set is now {:?}",
        report_view.sstable_ids(),
        manager.current_ids()
    );
    assert_eq!(
        report_view.sstable_ids(),
        &[1, 2, 3],
        "held view is isolated from later flushes"
    );
    assert_eq!(
        manager.current_ids(),
        vec![1, 2, 3, 4, 5],
        "the live manifest moved on"
    );
}

/// `lz4` feature: the hot tier reads constantly, so decompression sits on the
/// read hot path. LZ4 is the fast codec - a lower ratio for a cheaper decode.
/// Incompressible blocks fall back to a stored encoding so they never inflate.
#[cfg(feature = "lz4")]
fn lz4_hot_tier() {
    use subms_lsm_tree::Lz4BlockCompressor;
    println!("\n== lz4: fast compression for the hot tier ==");
    let codec = Lz4BlockCompressor::new();
    // A block of repeated fills, like a run of same-symbol ticks.
    let block = "AAPL,100@150.10;".repeat(256).into_bytes();
    let encoded = codec.compress(&block);
    println!("  {} bytes -> {} compressed", block.len(), encoded.len());
    assert!(encoded.len() < block.len(), "repetitive block shrinks");
    assert_eq!(
        codec.decompress(&encoded).unwrap(),
        block,
        "lossless round trip"
    );
}

/// `zstd` feature: the cold tier is written once and read rarely, so bytes on
/// disk dominate. Zstd trades CPU for a better ratio than LZ4 - the right deal
/// for archival runs.
#[cfg(feature = "zstd")]
fn zstd_cold_tier() {
    use subms_lsm_tree::ZstdBlockCompressor;
    println!("\n== zstd: higher-ratio compression for the cold tier ==");
    let codec = ZstdBlockCompressor::new();
    let block = "MSFT,50@320.55;".repeat(256).into_bytes();
    let encoded = codec.compress(&block).unwrap();
    println!(
        "  {} bytes -> {} compressed (level {})",
        block.len(),
        encoded.len(),
        codec.level()
    );
    assert!(encoded.len() < block.len(), "cold block shrinks");
    assert_eq!(
        codec.decompress(&encoded).unwrap(),
        block,
        "lossless round trip"
    );
}

/// `block-cache-integration` feature: a read-side LRU keyed on
/// `(sstable_id, block_offset)`. The read path consults it before touching
/// disk, so a hit skips the IO. Cached blocks are `Arc<[u8]>`, shared across
/// readers without a copy.
#[cfg(feature = "block-cache-integration")]
fn block_cache_read_path() {
    use subms_lsm_tree::{Block, BlockCache, BlockKey, LruBlockCache};
    println!("\n== block-cache-integration: read-side block cache ==");
    let cache = LruBlockCache::new(2);
    let hot = BlockKey::new(1, 0);

    assert!(cache.get(&hot).is_none(), "cold: a miss");
    cache.put(hot, Block::from(b"AAPL block".as_slice()));
    let served = cache.get(&hot).expect("warm: a hit");
    println!(
        "  {} hit / {} miss after one warm read",
        cache.hits(),
        cache.misses()
    );
    assert_eq!(&*served, b"AAPL block", "the cached payload is served");

    // A third distinct block evicts the least-recently-used entry (cap 2).
    cache.put(BlockKey::new(2, 0), Block::from(b"MSFT block".as_slice()));
    cache.put(BlockKey::new(3, 0), Block::from(b"GOOG block".as_slice()));
    assert!(
        cache.get(&hot).is_none(),
        "coldest block evicted at capacity"
    );
}
