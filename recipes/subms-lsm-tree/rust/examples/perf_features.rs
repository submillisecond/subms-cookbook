//! Feature classification bench. Each feature's representative op is swept
//! across three TREE SIZES, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! Live key count is the sweep axis because it is what sets everything an LSM
//! tree owns: the wal it has to replay, the entries a compaction has to rewrite,
//! the runs a snapshot pins, the blocks a cache has to hold. A per-op read is
//! size-independent and should read flat; anything that rewrites or rescans the
//! whole structure should climb with N.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness wal tiered-compaction leveled-compaction \
//!                   snapshot lz4 zstd block-cache-integration"

use std::collections::BTreeMap;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_lsm_tree::LsmTree;

/// 8k / 64k / 512k live keys, a 64x span. The bottom is deliberately not 1k:
/// at a thousand entries a wal replay or a run merge is mostly fixed per-call
/// cost, which compresses the measured ratio and reads as flat. The top is
/// where the merges actually cost something (a 512k-entry BTreeMap rebuild).
const SIZES: [usize; 3] = [8_192, 65_536, 524_288];
const CANON_N: usize = SIZES[SIZES.len() - 1];
/// Per-op reps. Fixed across the sweep so a slope has one cause.
const OPS: usize = 20_000;
/// Timed repeats for a whole-structure op, far too slow to run OPS times.
const BULK_REPS: usize = 32;
/// Bulk warmup is TIME-BOXED, not a fixed rep count. Rust has no JIT, but these
/// ops allocate hard (a merge builds a fresh BTreeMap; a replay builds a fresh
/// Vec of owned entries) and the allocator plus page-fault ramp does not settle
/// in a fixed handful of reps. A budget gives the cheap sizes thousands of
/// passes and the expensive ones as many as fit.
const BULK_WARM_NANOS: u64 = 300_000_000;
const BULK_WARM_MAX_REPS: usize = 5_000;
/// Per-op warm, also time-boxed, for the same reason and one more. A fixed
/// 20_000 reps is 24 ms of a block codec, which is not enough to settle an
/// allocator that has just had a few hundred megabytes of compaction templates
/// freed under it: lz4 read 2200 -> 1500 -> 1100 ns across a sweep whose axis it
/// does not even touch, and on the previous run the same artifact landed on
/// zstd instead. The op is capped as well as timed so a 200 ns planner call does
/// not spend the full budget.
const KEYED_WARM_NANOS: u64 = 300_000_000;
const KEYED_WARM_MAX_REPS: usize = 200_000;

/// One SSTable data block. Held CONSTANT across the sweep on purpose - see the
/// compression blocks below.
const BLOCK_BYTES: usize = 4096;
/// Entries per run in the compaction manifests. 128 runs at the top size.
const ENTRIES_PER_RUN: usize = 4_096;
/// Live keys behind one cached 4KB block at this recipe's entry size, so the
/// cache scales with the tree instead of staying a fixed 1024 slots.
const KEYS_PER_BLOCK: usize = 8;
/// Live keys per on-disk run, so a snapshot pins a manifest that grows with N.
const KEYS_PER_SSTABLE: usize = 4_096;
/// Big enough that the base tree ends up with ~20 runs rather than ~1300; a
/// read walking 1300 blooms measures the flush threshold, not the read path.
const FLUSH_BYTES: usize = 1_000_000;

const VALUE: &[u8] = b"value-payload-bytes-24ch";

/// Zero-padded so key order matches insertion order and a run's key range is a
/// contiguous interval - what leveled compaction's overlap selection assumes.
fn key(i: usize) -> String {
    format!("k{i:09}")
}

/// Spreads probes over the whole key space without a live rng in the timed loop.
fn probe(i: usize, n: usize) -> usize {
    (i.wrapping_mul(2_654_435_761)) % n
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// A per-op measurement, warmed over the same index range it then times.
fn keyed(mut op: impl FnMut(usize), median: bool) -> u64 {
    let start = std::time::Instant::now();
    for i in 0..KEYED_WARM_MAX_REPS {
        op(i % OPS);
        if start.elapsed().as_nanos() as u64 >= KEYED_WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("lsm-feature", "rust");
    let st = h.stage("op", OPS);
    for i in 0..OPS {
        st.time(|| op(i));
    }
    stat(&h, median)
}

/// A whole-structure op that leaves its input intact, so one setup serves every
/// rep. The warm pass is discarded: measured cold, a bulk op lands its
/// first-touch cost on whichever sweep point runs first, which reads as a curve
/// that FALLS with size - the opposite of the structural signal.
fn bulk(mut op: impl FnMut(), median: bool) -> u64 {
    let start = std::time::Instant::now();
    for _ in 0..BULK_WARM_MAX_REPS {
        op();
        if start.elapsed().as_nanos() as u64 >= BULK_WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("lsm-feature", "rust");
    let st = h.stage("op", BULK_REPS);
    for _ in 0..BULK_REPS {
        st.time(&mut op);
    }
    stat(&h, median)
}

/// A whole-structure op that CONSUMES its input. Both compaction entry points
/// take the runs out of the level they compact, so a second rep would merge an
/// empty level and the curve would read flat. `setup` rebuilds the input before
/// each rep, OUTSIDE the timed region - the alternative, rebuilding inside the
/// closure, publishes the manifest build as if it were the merge.
fn bulk_each<T>(mut setup: impl FnMut() -> T, mut op: impl FnMut(&mut T), median: bool) -> u64 {
    let start = std::time::Instant::now();
    for _ in 0..BULK_WARM_MAX_REPS {
        let mut input = setup();
        op(&mut input);
        if start.elapsed().as_nanos() as u64 >= BULK_WARM_NANOS {
            break;
        }
    }
    let mut h = SubMsPerfHarness::new("lsm-feature", "rust");
    let st = h.stage("op", BULK_REPS);
    for _ in 0..BULK_REPS {
        let mut input = setup();
        st.time(|| op(&mut input));
    }
    stat(&h, median)
}

/// Sweeps and PRINTS the curve. A ratio-compressed or non-monotonic curve
/// classifies flat and the rows are the only place it shows.
fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = SIZES.iter().map(|&n| (n, at(n))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

fn main() -> io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);
    // Stamp the box these numbers came from. The bench runs wherever it is
    // invoked, so an unstamped manifest is indistinguishable from a fleet
    // capture; the renderer will not publish one it cannot attribute.
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    let tmp = TempDir::new("subms-lsm-features");

    // The baseline: a base `get` against a real tree at the canonical size.
    // Every feature is classified against the cost of the read it decorates.
    let base_p50 = {
        let mut tree = LsmTree::open(tmp.path().join("base"), FLUSH_BYTES)?;
        for i in 0..CANON_N {
            tree.put(&key(i), VALUE)?;
        }
        tree.flush()?;
        let probes: Vec<String> = (0..OPS).map(|i| key(probe(i, CANON_N))).collect();
        keyed(
            |i| {
                black_box(tree.get(&probes[i]).expect("get"));
            },
            true,
        )
    };
    eprintln!("base get p50: {base_p50}ns ({CANON_N} live keys)");

    #[cfg(feature = "wal")]
    feature_wal(&mut manifest, base_p50, tmp.path());

    #[cfg(feature = "tiered-compaction")]
    feature_tiered(&mut manifest, base_p50);

    #[cfg(feature = "leveled-compaction")]
    feature_leveled(&mut manifest, base_p50);

    #[cfg(feature = "snapshot")]
    feature_snapshot(&mut manifest, base_p50);

    #[cfg(feature = "lz4")]
    feature_lz4(&mut manifest, base_p50);

    #[cfg(feature = "zstd")]
    feature_zstd(&mut manifest, base_p50);

    #[cfg(feature = "block-cache-integration")]
    feature_block_cache(&mut manifest, base_p50);

    drop(tmp);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}

// ---------- wal: durable append, whole-log replay ----------

/// Writes `n` put records and returns the log path.
#[cfg(feature = "wal")]
fn wal_of(dir: &Path, n: usize) -> PathBuf {
    use subms_lsm_tree::WriteAheadLog;
    let path = dir.join(format!("replay-{n}.wal"));
    let _ = std::fs::remove_file(&path);
    let mut wal = WriteAheadLog::open(&path).expect("open wal");
    for i in 0..n {
        wal.log_put(&key(i), VALUE).expect("log_put");
    }
    wal.sync().expect("sync");
    path
}

#[cfg(feature = "wal")]
fn feature_wal(manifest: &mut SubMsFeatureManifest, base_p50: u64, dir: &Path) {
    use subms_lsm_tree::WriteAheadLog;

    // Swept on `replay`, not on `log_put`. The append is one buffered write per
    // record and is flat by construction; recovery - reading and CRC-verifying
    // every record written since the last flush - is what the wal EXISTS for,
    // and it is the part that grows with the tree.
    let sw = sweep("wal/replay", |n| {
        let path = wal_of(dir, n);
        let out = bulk(
            || {
                black_box(WriteAheadLog::replay(&path).expect("replay").len());
            },
            true,
        );
        let _ = std::fs::remove_file(&path);
        out
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let replay_path = wal_of(dir, CANON_N);
    let mut p99 = BTreeMap::new();
    p99.insert(
        "replay".to_string(),
        bulk(
            || {
                black_box(WriteAheadLog::replay(&replay_path).expect("replay").len());
            },
            false,
        ),
    );
    let _ = std::fs::remove_file(&replay_path);

    // A scratch log, so the append measurement does not inflate the one above.
    let append_path = dir.join("append.wal");
    let _ = std::fs::remove_file(&append_path);
    {
        let mut wal = WriteAheadLog::open(&append_path).expect("open wal");
        let keys: Vec<String> = (0..OPS).map(key).collect();
        p99.insert(
            "log_put".to_string(),
            keyed(|i| wal.log_put(&keys[i], VALUE).expect("log_put"), false),
        );
    }
    let _ = std::fs::remove_file(&append_path);
    // `sync` is deliberately absent from both the sweep and the stage table.
    // fsync is a device property - tens of us on battery-backed NVMe, single-
    // digit ms on this laptop tier - so a number for it would move with the
    // hardware under a column the reader reads as the cost of the code, and
    // sweeping it would dress a constant storage-stack cost as a scaling result.
    manifest.set_feature("wal", cat, &p99, &reason);
}

// ---------- tiered-compaction: merge every run at a level into one ----------

#[cfg(feature = "tiered-compaction")]
fn feature_tiered(manifest: &mut SubMsFeatureManifest, base_p50: u64) {
    use subms_lsm_tree::{TieredCompactionPlanner, TieredManifest, TieredRun};

    // One template per size, deep-cloned per rep. Cloning is the same order of
    // work as building the runs from scratch but skips re-formatting 512k keys.
    fn runs(n: usize) -> Vec<TieredRun> {
        (0..n.div_ceil(ENTRIES_PER_RUN))
            .map(|r| {
                let entries: Vec<(String, Option<Vec<u8>>)> = (0..ENTRIES_PER_RUN)
                    .map(|j| (key(r * ENTRIES_PER_RUN + j), Some(VALUE.to_vec())))
                    .collect();
                TieredRun::new(r as u64, entries)
            })
            .collect()
    }

    let planner = TieredCompactionPlanner::new(2);
    // Swept on `merge`, not on `pick_level`. Picking a level is a scan of the
    // per-level run counts and is O(levels); the merge is the whole point of
    // the feature and rewrites every entry at the level.
    let sw = sweep("tiered-compaction/merge", |n| {
        let template = runs(n);
        bulk_each(
            || TieredManifest {
                levels: vec![template.clone()],
            },
            |m| planner.merge(m, 0, 9_999),
            true,
        )
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let template = runs(CANON_N);
    let mut p99 = BTreeMap::new();
    p99.insert(
        "merge".to_string(),
        bulk_each(
            || TieredManifest {
                levels: vec![template.clone()],
            },
            |m| planner.merge(m, 0, 9_999),
            false,
        ),
    );
    let planned = TieredManifest {
        levels: vec![template],
    };
    p99.insert(
        "plan".to_string(),
        keyed(|_| _ = black_box(planner.pick_level(&planned)), false),
    );
    manifest.set_feature("tiered-compaction", cat, &p99, &reason);
}

// ---------- leveled-compaction: merge L0 into the overlapping L1 runs ----------

#[cfg(feature = "leveled-compaction")]
fn feature_leveled(manifest: &mut SubMsFeatureManifest, base_p50: u64) {
    use subms_lsm_tree::{LeveledCompactionPlanner, LeveledManifest, LeveledRun};

    // L0 and L1 interleave over the same key space (even indices vs odd), so
    // every L1 run overlaps the L0 range and the compaction picks all of them
    // up. Disjoint halves would leave L1 untouched and the merge would only
    // ever rewrite half the tree.
    fn halves(n: usize) -> (Vec<LeveledRun>, Vec<LeveledRun>) {
        let per_level = n / 2;
        let build = |parity: usize| -> Vec<LeveledRun> {
            (0..per_level.div_ceil(ENTRIES_PER_RUN))
                .map(|r| {
                    let entries: Vec<(String, Option<Vec<u8>>)> = (0..ENTRIES_PER_RUN)
                        .map(|j| {
                            let idx = 2 * (r * ENTRIES_PER_RUN + j) + parity;
                            (key(idx), Some(VALUE.to_vec()))
                        })
                        .collect();
                    LeveledRun::new((r * 2 + parity) as u64, entries)
                })
                .collect()
        };
        (build(0), build(1))
    }

    let planner = LeveledCompactionPlanner::new(64_000, 10, 4);
    // Swept on `compact`, not on `pick_level`. The budget scan is O(runs); the
    // compaction rewrites every entry it touches.
    let sw = sweep("leveled-compaction/compact", |n| {
        let (l0, l1) = halves(n);
        bulk_each(
            || LeveledManifest {
                levels: vec![l0.clone(), l1.clone()],
            },
            |m| planner.compact(m, 0, 9_999),
            true,
        )
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let (l0, l1) = halves(CANON_N);
    let mut p99 = BTreeMap::new();
    p99.insert(
        "compact".to_string(),
        bulk_each(
            || LeveledManifest {
                levels: vec![l0.clone(), l1.clone()],
            },
            |m| planner.compact(m, 0, 9_999),
            false,
        ),
    );
    let planned = LeveledManifest {
        levels: vec![l0, l1],
    };
    p99.insert(
        "plan".to_string(),
        keyed(|_| _ = black_box(planner.pick_level(&planned)), false),
    );
    manifest.set_feature("leveled-compaction", cat, &p99, &reason);
}

// ---------- snapshot: pinned point-in-time view of the run manifest ----------

#[cfg(feature = "snapshot")]
fn feature_snapshot(manifest: &mut SubMsFeatureManifest, base_p50: u64) {
    use subms_lsm_tree::{SnapshotManager, SnapshotManifest};

    fn manager(n: usize) -> SnapshotManager {
        let ids: Vec<u64> = (0..(n / KEYS_PER_SSTABLE).max(1) as u64).collect();
        SnapshotManager::with_initial(SnapshotManifest::new(ids))
    }

    // The manifest under test grows with the tree - 2 run ids at the bottom
    // size, 128 at the top - which is what makes a flat result mean something.
    // Taking a snapshot is an Arc bump and an id increment behind two short
    // mutex sections, so it should not care, and the sweep is how that is shown
    // rather than asserted.
    let sw = sweep("snapshot/snapshot", |n| {
        let mgr = manager(n);
        keyed(|_| _ = black_box(mgr.snapshot()), true)
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let mgr = manager(CANON_N);
    let mut p99 = BTreeMap::new();
    p99.insert(
        "snapshot".to_string(),
        keyed(|_| _ = black_box(mgr.snapshot()), false),
    );
    // The read side: resolve a key against a held view by walking its pinned run
    // ids newest-first, the order the tree's own read path uses.
    let held = mgr.snapshot();
    let ids = held.sstable_ids();
    let targets: Vec<u64> = (0..OPS)
        .map(|i| probe(i, ids.len().max(1) * 2) as u64)
        .collect();
    p99.insert(
        "get_on_snapshot".to_string(),
        keyed(
            |i| {
                let t = targets[i];
                black_box(ids.iter().rev().any(|&id| id == t));
            },
            false,
        ),
    );
    manifest.set_feature("snapshot", cat, &p99, &reason);
}

// ---------- lz4 / zstd: SSTable block compression ----------

/// A representative ~4KB SSTable data block: repeating record-shaped text so
/// the compressors have realistic-but-not-degenerate redundancy.
#[cfg(any(feature = "lz4", feature = "zstd", feature = "block-cache-integration"))]
fn representative_block() -> Vec<u8> {
    let pattern = b"key-0000042\x00present\x00value-payload-bytes-for-block|";
    let mut out = Vec::with_capacity(BLOCK_BYTES + pattern.len());
    while out.len() < BLOCK_BYTES {
        out.extend_from_slice(pattern);
    }
    out.truncate(BLOCK_BYTES);
    out
}

#[cfg(feature = "lz4")]
fn feature_lz4(manifest: &mut SubMsFeatureManifest, base_p50: u64) {
    use subms_lsm_tree::Lz4BlockCompressor;

    let c = Lz4BlockCompressor::new();
    // The block is held at BLOCK_BYTES at EVERY sweep point. Compression cost
    // tracks the bytes handed to the codec, so growing the block with the tree
    // would publish a payload sweep dressed as a tree-size sweep - and an LSM
    // block size is a configuration constant, not a function of how many keys
    // are live. The flat curve is the finding: a bigger tree is more blocks at
    // the same per-block cost, not a more expensive block.
    let block = representative_block();
    let sw = sweep("lz4/compress", |_| {
        keyed(|_| _ = black_box(c.compress(&block)), true)
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let encoded = c.compress(&block);
    let mut p99 = BTreeMap::new();
    p99.insert(
        "compress_block".to_string(),
        keyed(|_| _ = black_box(c.compress(&block)), false),
    );
    p99.insert(
        "decompress_block".to_string(),
        keyed(
            |_| _ = black_box(c.decompress(&encoded).expect("lz4 decode")),
            false,
        ),
    );
    manifest.set_feature("lz4", cat, &p99, &reason);
}

#[cfg(feature = "zstd")]
fn feature_zstd(manifest: &mut SubMsFeatureManifest, base_p50: u64) {
    use subms_lsm_tree::ZstdBlockCompressor;

    let c = ZstdBlockCompressor::new();
    let block = representative_block();
    let sw = sweep("zstd/compress", |_| {
        keyed(
            |_| _ = black_box(c.compress(&block).expect("zstd encode")),
            true,
        )
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let encoded = c.compress(&block).expect("zstd encode");
    let mut p99 = BTreeMap::new();
    p99.insert(
        "compress_block".to_string(),
        keyed(
            |_| _ = black_box(c.compress(&block).expect("zstd encode")),
            false,
        ),
    );
    p99.insert(
        "decompress_block".to_string(),
        keyed(
            |_| _ = black_box(c.decompress(&encoded).expect("zstd decode")),
            false,
        ),
    );
    manifest.set_feature("zstd", cat, &p99, &reason);
}

// ---------- block-cache-integration: read-path block cache ----------

#[cfg(feature = "block-cache-integration")]
fn feature_block_cache(manifest: &mut SubMsFeatureManifest, base_p50: u64) {
    use std::sync::Arc;
    use subms_lsm_tree::{Block, BlockCache, BlockKey, LruBlockCache};

    // Capacity scales with the tree and the cache is filled to it, so the
    // occupied fraction is the same at every sweep point. A fixed 1024 slots
    // would hold the hash map at one size while claiming to sweep the tree.
    // One shared `Arc<[u8]>` payload keeps 64k cached blocks in memory instead
    // of 256 MB of identical bytes; the cache stores the pointer either way.
    fn filled(n: usize) -> (LruBlockCache, usize) {
        let cap = (n / KEYS_PER_BLOCK).max(64);
        let cache = LruBlockCache::new(cap);
        let block: Block = Arc::from(representative_block().into_boxed_slice());
        for i in 0..cap as u64 {
            cache.put(BlockKey::new(i % 8, i * BLOCK_BYTES as u64), block.clone());
        }
        (cache, cap)
    }

    let sw = sweep("block-cache-integration/get_cached", |n| {
        let (cache, cap) = filled(n);
        let keys: Vec<BlockKey> = (0..OPS)
            .map(|i| {
                let k = probe(i, cap) as u64;
                BlockKey::new(k % 8, k * BLOCK_BYTES as u64)
            })
            .collect();
        keyed(|i| _ = black_box(cache.get(&keys[i])), true)
    });
    let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

    let (cache, cap) = filled(CANON_N);
    let hits: Vec<BlockKey> = (0..OPS)
        .map(|i| {
            let k = probe(i, cap) as u64;
            BlockKey::new(k % 8, k * BLOCK_BYTES as u64)
        })
        .collect();
    let misses: Vec<BlockKey> = (0..OPS)
        .map(|i| BlockKey::new(999, probe(i, cap) as u64))
        .collect();
    let mut p99 = BTreeMap::new();
    p99.insert(
        "get_cached".to_string(),
        keyed(|i| _ = black_box(cache.get(&hits[i])), false),
    );
    p99.insert(
        "get_miss".to_string(),
        keyed(|i| _ = black_box(cache.get(&misses[i])), false),
    );
    manifest.set_feature("block-cache-integration", cat, &p99, &reason);
}

/// Minimal unique-per-process temp dir with best-effort cleanup on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}-{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
