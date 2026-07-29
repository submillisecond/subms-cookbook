//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base order-journal scenario resolves newest-wins with tombstones and a
//! bloom-accelerated miss, and each opt-in feature holds the property its
//! section leans on. Feature-gated the same way as the sample; std-only.

use std::env;
use std::io;
use std::path::PathBuf;

use subms_lsm_tree::LsmTree;

fn scratch_dir(label: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lsm-sample-test-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn order_journal_scenario() -> io::Result<()> {
    let dir = scratch_dir("base");
    let mut journal = LsmTree::open(&dir, 256)?;

    journal.put("ORD-0001", b"AAPL,100@150.10")?;
    journal.put("ORD-0002", b"MSFT,50@320.55")?;
    journal.put("ORD-0003", b"GOOG,25@140.20")?;
    journal.flush()?;

    journal.put("ORD-0001", b"AAPL,100@150.42")?;
    journal.put("ORD-0004", b"NVDA,10@900.00")?;
    journal.delete("ORD-0002")?;
    journal.flush()?;

    assert_eq!(
        journal.get("ORD-0001")?.as_deref(),
        Some(&b"AAPL,100@150.42"[..]),
        "newest write wins"
    );
    assert!(
        journal.get("ORD-0002")?.is_none(),
        "cancelled order reads absent"
    );
    assert!(
        journal.get("ORD-9999")?.is_none(),
        "unknown id is a bloom-accelerated miss"
    );

    let book = journal.range(Some("ORD-0001"), Some("ORD-0005"))?;
    let live_ids: Vec<&str> = book.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        live_ids,
        ["ORD-0001", "ORD-0003", "ORD-0004"],
        "sorted, tombstone dropped"
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[cfg(feature = "wal")]
#[test]
fn wal_replay_recovers_acked_writes() -> io::Result<()> {
    use subms_lsm_tree::WriteAheadLog;
    let dir = scratch_dir("wal");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("journal.wal");
    {
        let mut wal = WriteAheadLog::open(&path)?;
        wal.log_put("ORD-0100", b"AAPL,100@150.10")?;
        wal.log_put("ORD-0101", b"MSFT,50@320.55")?;
        wal.log_delete("ORD-0100")?;
        wal.sync()?;
    }
    let recovered = WriteAheadLog::replay(&path)?;
    assert_eq!(recovered.len(), 3, "every acked write survives the crash");
    assert!(
        recovered[2].value.is_none(),
        "the cancel replays as a tombstone"
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[cfg(feature = "tiered-compaction")]
#[test]
fn tiered_merge_promotes_full_level() {
    use subms_lsm_tree::{TieredCompactionPlanner, TieredManifest, TieredRun};
    let mut manifest = TieredManifest::new();
    for i in 0..4 {
        manifest.push(
            0,
            TieredRun::new(i, vec![(format!("ORD-{i:04}"), Some(b"fill".to_vec()))]),
        );
    }
    let planner = TieredCompactionPlanner::new(4);
    let level = planner.pick_level(&manifest).unwrap();
    planner.merge(&mut manifest, level, 100);
    assert_eq!(manifest.level_run_count(0), 0, "L0 drained");
    assert_eq!(manifest.level_run_count(1), 1, "merged run promoted to L1");
    assert!(
        planner.pick_level(&manifest).is_none(),
        "single run does not re-trigger"
    );
}

#[cfg(feature = "leveled-compaction")]
#[test]
fn leveled_compaction_keeps_levels_disjoint() {
    use subms_lsm_tree::features::leveled_compaction::level_is_non_overlapping;
    use subms_lsm_tree::{LeveledCompactionPlanner, LeveledManifest, LeveledRun};
    let mut manifest = LeveledManifest::new();
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
                ("AAPL".to_string(), Some(b"149.00".to_vec())),
                ("NVDA".to_string(), Some(b"900.00".to_vec())),
            ],
        ),
    );
    let planner = LeveledCompactionPlanner::new(1_000_000, 10, 2);
    let from = planner.pick_level(&manifest).unwrap();
    planner.compact(&mut manifest, from, 100);
    assert_eq!(manifest.level_run_count(0), 0, "L0 drained into L1");
    assert!(
        level_is_non_overlapping(&manifest, 1),
        "L1 key-disjoint after compaction"
    );
}

#[cfg(feature = "snapshot")]
#[test]
fn snapshot_is_isolated_from_later_publishes() {
    use subms_lsm_tree::{SnapshotManager, SnapshotManifest};
    let manager = SnapshotManager::new();
    manager.publish(SnapshotManifest::new(vec![1, 2, 3]));
    let view = manager.snapshot();
    manager.publish(SnapshotManifest::new(vec![1, 2, 3, 4, 5]));
    assert_eq!(
        view.sstable_ids(),
        &[1, 2, 3],
        "held view unchanged by later flush"
    );
    assert_eq!(
        manager.current_ids(),
        vec![1, 2, 3, 4, 5],
        "live manifest moved on"
    );
}

#[cfg(feature = "lz4")]
#[test]
fn lz4_round_trips_and_shrinks() {
    use subms_lsm_tree::Lz4BlockCompressor;
    let codec = Lz4BlockCompressor::new();
    let block = "AAPL,100@150.10;".repeat(256).into_bytes();
    let encoded = codec.compress(&block);
    assert!(encoded.len() < block.len(), "repetitive block shrinks");
    assert_eq!(
        codec.decompress(&encoded).unwrap(),
        block,
        "lossless round trip"
    );
}

#[cfg(feature = "zstd")]
#[test]
fn zstd_round_trips_and_shrinks() {
    use subms_lsm_tree::ZstdBlockCompressor;
    let codec = ZstdBlockCompressor::new();
    let block = "MSFT,50@320.55;".repeat(256).into_bytes();
    let encoded = codec.compress(&block).unwrap();
    assert!(encoded.len() < block.len(), "cold block shrinks");
    assert_eq!(
        codec.decompress(&encoded).unwrap(),
        block,
        "lossless round trip"
    );
}

#[cfg(feature = "block-cache-integration")]
#[test]
fn block_cache_serves_hits_and_evicts_lru() {
    use subms_lsm_tree::{Block, BlockCache, BlockKey, LruBlockCache};
    let cache = LruBlockCache::new(2);
    let hot = BlockKey::new(1, 0);
    assert!(cache.get(&hot).is_none(), "cold miss");
    cache.put(hot, Block::from(b"AAPL block".as_slice()));
    assert_eq!(
        &*cache.get(&hot).unwrap(),
        b"AAPL block",
        "warm hit serves the payload"
    );
    cache.put(BlockKey::new(2, 0), Block::from(b"MSFT block".as_slice()));
    cache.put(BlockKey::new(3, 0), Block::from(b"GOOG block".as_slice()));
    assert!(cache.get(&hot).is_none(), "coldest evicted at capacity");
    assert_eq!(cache.hits(), 1);
}
