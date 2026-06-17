//! Per-feature bench: one stage block per opt-in feature, measuring the
//! per-op cost the cookbook page reports. The base LSM read/write path lives
//! in `perf_main`; this example covers only the feature modules behind their
//! Cargo flags.
//!
//! Stages (each enabled only when its feature compiles in):
//!   wal:                       wal_put, wal_replay
//!   tiered-compaction:         tiered_plan
//!   leveled-compaction:        leveled_plan
//!   snapshot:                  snapshot, get_on_snapshot
//!   lz4:                       lz4_compress_block, lz4_decompress_block
//!   zstd:                      zstd_compress_block, zstd_decompress_block
//!   block-cache-integration:   cache_get_cached, cache_get_miss
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness wal tiered-compaction leveled-compaction \
//!                   snapshot lz4 zstd block-cache-integration"

use std::io::{self, Write};

use subms::{SubMsLcg, SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;
const BLOCK_BYTES: usize = 4096;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("lsm-tree-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.input("block_bytes", &BLOCK_BYTES.to_string());
    h.add_meta("subms.recipe.slug", "subms-lsm-tree");
    h.add_meta("subms.recipe.category", "storage");

    let tmp = TempDir::new("subms-lsm-features");

    #[cfg(feature = "wal")]
    bench_wal(&mut h, tmp.path());

    #[cfg(feature = "tiered-compaction")]
    bench_tiered(&mut h);

    #[cfg(feature = "leveled-compaction")]
    bench_leveled(&mut h);

    #[cfg(feature = "snapshot")]
    bench_snapshot(&mut h);

    #[cfg(feature = "lz4")]
    bench_lz4(&mut h);

    #[cfg(feature = "zstd")]
    bench_zstd(&mut h);

    #[cfg(feature = "block-cache-integration")]
    bench_block_cache(&mut h);

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;

    drop(tmp);
    Ok(())
}

/// A representative ~4KB SSTable data block: repeating record-shaped text so
/// the compressors have realistic-but-not-degenerate redundancy.
fn representative_block() -> Vec<u8> {
    let pattern = b"key-0000042\x00present\x00value-payload-bytes-for-block|";
    let mut out = Vec::with_capacity(BLOCK_BYTES + pattern.len());
    while out.len() < BLOCK_BYTES {
        out.extend_from_slice(pattern);
    }
    out.truncate(BLOCK_BYTES);
    out
}

#[cfg(feature = "wal")]
fn bench_wal(h: &mut SubMsPerfHarness, dir: &std::path::Path) {
    use subms_lsm_tree::WriteAheadLog;
    h.add_meta("subms.workload.feature", "wal");

    let path = dir.join("bench.wal");
    let _ = std::fs::remove_file(&path);

    // wal_put: append cost per write (buffered + flush, no fsync).
    {
        let mut wal = WriteAheadLog::open(&path).expect("open wal");
        let mut rng = SubMsLcg::new(SEED);
        let stage = h
            .stage("wal_put", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let key = format!("k{}", rng.next_u32());
            stage.time(|| wal.log_put(&key, b"value-payload-bytes").expect("log_put"));
        }
        wal.sync().expect("sync");
    }

    // wal_replay: whole-log scan + CRC verify. One replay reads the entire
    // populated log, so each sample is a full ENTRIES-record recovery; repeat
    // to get a distribution.
    {
        const REPLAYS: usize = 200;
        let stage = h
            .stage("wal_replay", REPLAYS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..REPLAYS {
            stage.time(|| {
                let entries = WriteAheadLog::replay(&path).expect("replay");
                std::hint::black_box(entries.len());
            });
        }
    }

    let _ = std::fs::remove_file(&path);
}

#[cfg(feature = "tiered-compaction")]
fn bench_tiered(h: &mut SubMsPerfHarness) {
    use subms_lsm_tree::{TieredCompactionPlanner, TieredManifest, TieredRun};
    h.add_meta("subms.workload.feature", "tiered-compaction");

    let mut manifest = TieredManifest::new();
    let mut rng = SubMsLcg::new(SEED);
    for id in 0..50u64 {
        let entries: Vec<(String, Option<Vec<u8>>)> = (0..32)
            .map(|_| (format!("k{}", rng.next_u32()), Some(b"v".to_vec())))
            .collect();
        manifest.push(0, TieredRun::new(id, entries));
    }
    let planner = TieredCompactionPlanner::new(50);

    // tiered_plan: scan the manifest and decide which level to compact.
    let stage = h
        .stage("tiered_plan", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..ENTRIES {
        stage.time(|| std::hint::black_box(planner.pick_level(&manifest)));
    }
}

#[cfg(feature = "leveled-compaction")]
fn bench_leveled(h: &mut SubMsPerfHarness) {
    use subms_lsm_tree::{LeveledCompactionPlanner, LeveledManifest, LeveledRun};
    h.add_meta("subms.workload.feature", "leveled-compaction");

    let mut manifest = LeveledManifest::new();
    let mut rng = SubMsLcg::new(SEED);
    // L0 holds a few overlapping runs; L1+ hold disjoint runs sorted by key.
    for id in 0..4u64 {
        let entries: Vec<(String, Option<Vec<u8>>)> = (0..32)
            .map(|_| (format!("k{}", rng.next_u32()), Some(b"v".to_vec())))
            .collect();
        manifest.push(0, LeveledRun::new(id, entries));
    }
    for id in 4..50u64 {
        let base = (id - 4) * 1000;
        let entries: Vec<(String, Option<Vec<u8>>)> = (0..32)
            .map(|j| (format!("k{:08}", base + j), Some(b"v".to_vec())))
            .collect();
        manifest.push(1, LeveledRun::new(id, entries));
    }
    let planner = LeveledCompactionPlanner::new(64_000, 10, 4);

    // leveled_plan: L0-run-limit check + per-level byte-budget scan.
    let stage = h
        .stage("leveled_plan", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..ENTRIES {
        stage.time(|| std::hint::black_box(planner.pick_level(&manifest)));
    }
}

#[cfg(feature = "snapshot")]
fn bench_snapshot(h: &mut SubMsPerfHarness) {
    use subms_lsm_tree::{SnapshotManager, SnapshotManifest};
    h.add_meta("subms.workload.feature", "snapshot");

    let ids: Vec<u64> = (0..50).collect();
    let mgr = SnapshotManager::with_initial(SnapshotManifest::new(ids));

    // snapshot: Arc bump + id allocation under two short mutex sections.
    {
        let stage = h
            .stage("snapshot", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| {
                std::hint::black_box(mgr.snapshot());
            });
        }
    }

    // get_on_snapshot: a read resolving against a held snapshot's manifest -
    // walk the captured SSTable id list newest-to-oldest looking for a target.
    {
        let snap = mgr.snapshot();
        let mut rng = SubMsLcg::new(SEED);
        let stage = h
            .stage("get_on_snapshot", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let target = rng.bounded(60) as u64;
            stage.time(|| {
                let found = snap.sstable_ids().iter().rev().any(|&id| id == target);
                std::hint::black_box(found);
            });
        }
    }
}

#[cfg(feature = "lz4")]
fn bench_lz4(h: &mut SubMsPerfHarness) {
    use subms_lsm_tree::Lz4BlockCompressor;
    h.add_meta("subms.workload.feature", "lz4");

    let c = Lz4BlockCompressor::new();
    let block = representative_block();
    let encoded = c.compress(&block);

    {
        let stage = h
            .stage("lz4_compress_block", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| std::hint::black_box(c.compress(&block)));
        }
    }
    {
        let stage = h
            .stage("lz4_decompress_block", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| std::hint::black_box(c.decompress(&encoded).expect("lz4 decode")));
        }
    }
}

#[cfg(feature = "zstd")]
fn bench_zstd(h: &mut SubMsPerfHarness) {
    use subms_lsm_tree::ZstdBlockCompressor;
    h.add_meta("subms.workload.feature", "zstd");

    let c = ZstdBlockCompressor::new();
    let block = representative_block();
    let encoded = c.compress(&block).expect("zstd encode");

    {
        let stage = h
            .stage("zstd_compress_block", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| std::hint::black_box(c.compress(&block).expect("zstd encode")));
        }
    }
    {
        let stage = h
            .stage("zstd_decompress_block", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| std::hint::black_box(c.decompress(&encoded).expect("zstd decode")));
        }
    }
}

#[cfg(feature = "block-cache-integration")]
fn bench_block_cache(h: &mut SubMsPerfHarness) {
    use std::sync::Arc;
    use subms_lsm_tree::{Block, BlockCache, BlockKey, LruBlockCache};
    h.add_meta("subms.workload.feature", "block-cache-integration");

    const CACHE_CAP: usize = 1024;
    let cache = LruBlockCache::new(CACHE_CAP);
    let block: Block = Arc::from(representative_block().into_boxed_slice());
    for i in 0..CACHE_CAP as u64 {
        cache.put(BlockKey::new(i % 8, i * BLOCK_BYTES as u64), block.clone());
    }

    // cache_get_cached: warm hit - lock, map lookup, LRU move-to-front.
    {
        let mut rng = SubMsLcg::new(SEED);
        let stage = h
            .stage("cache_get_cached", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let i = rng.bounded(CACHE_CAP as u32) as u64;
            let key = BlockKey::new(i % 8, i * BLOCK_BYTES as u64);
            stage.time(|| std::hint::black_box(cache.get(&key)));
        }
    }

    // cache_get_miss: lock, map lookup, miss-counter bump.
    {
        let mut rng = SubMsLcg::new(SEED ^ 0x9E37_79B9);
        let stage = h
            .stage("cache_get_miss", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            let key = BlockKey::new(999, rng.next_u32() as u64);
            stage.time(|| std::hint::black_box(cache.get(&key)));
        }
    }
}

/// Minimal unique-per-process temp dir with best-effort cleanup on drop.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}-{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
