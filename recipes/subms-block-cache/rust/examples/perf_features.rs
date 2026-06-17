//! Per-feature bench: runs the same 50k-entry workload against the base
//! `BlockCache`, plus each opt-in feature (`arc`, `tinylfu`, `weighted`,
//! `concurrent-shards`, `metrics`) when its Cargo feature is enabled at
//! compile time.
//!
//! Each cache is pre-populated to its capacity, then we time two stages:
//! `*_get_hit` reads keys drawn from the resident set, and `*_put`
//! inserts fresh keys against a full cache so every put drives an
//! eviction. The output JSON has one stage block per feature variant -
//! `base_get_hit`, `arc_put`, etc. - so the cookbook page can fill the
//! per-feature p99 table from a single JSON file.
//!
//! `clock-sweep` is not a feature here: it IS the base cache, so the
//! `base_*` stages already measure it.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness arc tinylfu weighted concurrent-shards metrics"

use std::io::{self, Write};

use subms::{SubMsLcg, SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;

/// Time `count` reads of keys drawn from the resident set `0..ENTRIES`.
fn bench_get_hit<F>(h: &mut SubMsPerfHarness, name: &str, mut get: F)
where
    F: FnMut(u32) -> bool,
{
    let mut rng = SubMsLcg::new(SEED);
    let stage = h.stage(name, ENTRIES).with_kind(SubMsStageKind::HotPath);
    for _ in 0..ENTRIES {
        let key = rng.next_u32() % (ENTRIES as u32);
        stage.time(|| {
            let _ = get(key);
        });
    }
}

/// Time `count` inserts of fresh keys against a full cache; each put
/// drives an eviction.
fn bench_put<F>(h: &mut SubMsPerfHarness, name: &str, mut put: F)
where
    F: FnMut(u32),
{
    let base = ENTRIES as u32;
    let stage = h.stage(name, ENTRIES).with_kind(SubMsStageKind::HotPath);
    for i in 0..ENTRIES {
        let key = base + i as u32;
        stage.time(|| put(key));
    }
}

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("block-cache-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-block-cache");
    h.add_meta("subms.recipe.category", "memory");

    // ---------- base (clock-sweep) ----------
    {
        use subms_block_cache::BlockCache;
        h.add_meta("subms.workload.feature", "base");
        let mut c: BlockCache<u32, u64> = BlockCache::with_capacity(ENTRIES);
        for k in 0..ENTRIES as u32 {
            c.put(k, k as u64);
        }
        bench_get_hit(&mut h, "base_get_hit", |key| c.get(&key).is_some());
        bench_put(&mut h, "base_put", |key| {
            c.put(key, key as u64);
        });
    }

    // ---------- arc ----------
    #[cfg(feature = "arc")]
    {
        use subms_block_cache::ArcCache;
        h.add_meta("subms.workload.feature", "arc");
        let mut c: ArcCache<u32, u64> = ArcCache::with_capacity(ENTRIES);
        for k in 0..ENTRIES as u32 {
            c.put(k, k as u64);
        }
        bench_get_hit(&mut h, "arc_get_hit", |key| c.get(&key).is_some());
        bench_put(&mut h, "arc_put", |key| {
            c.put(key, key as u64);
        });
    }

    // ---------- tinylfu ----------
    #[cfg(feature = "tinylfu")]
    {
        use subms_block_cache::TinyLfuCache;
        h.add_meta("subms.workload.feature", "tinylfu");
        let mut c: TinyLfuCache<u32, u64> = TinyLfuCache::with_capacity(ENTRIES);
        for k in 0..ENTRIES as u32 {
            c.put(k, k as u64);
        }
        bench_get_hit(&mut h, "tinylfu_get_hit", |key| c.get(&key).is_some());
        bench_put(&mut h, "tinylfu_put", |key| {
            c.put(key, key as u64);
        });
    }

    // ---------- weighted ----------
    #[cfg(feature = "weighted")]
    {
        use subms_block_cache::WeightedCache;
        h.add_meta("subms.workload.feature", "weighted");
        // 1 byte per entry so capacity_bytes == slot capacity; eviction
        // behaves like the base cache on puts.
        let mut c: WeightedCache<u32, u64> =
            WeightedCache::with_capacity_bytes(ENTRIES, |_v: &u64| 1);
        for k in 0..ENTRIES as u32 {
            c.put(k, k as u64);
        }
        bench_get_hit(&mut h, "weighted_get_hit", |key| c.get(&key).is_some());
        bench_put(&mut h, "weighted_put", |key| {
            let _ = c.put(key, key as u64);
        });
    }

    // ---------- concurrent-shards (single-threaded) ----------
    #[cfg(feature = "concurrent-shards")]
    {
        use subms_block_cache::ShardedCache;
        h.add_meta("subms.workload.feature", "concurrent-shards");
        let c: ShardedCache<u32, u64> = ShardedCache::with_capacity(ENTRIES, 16);
        for k in 0..ENTRIES as u32 {
            c.put(k, k as u64);
        }
        bench_get_hit(&mut h, "concurrent_shards_get_hit", |key| {
            c.get(&key).is_some()
        });
        bench_put(&mut h, "concurrent_shards_put", |key| {
            c.put(key, key as u64);
        });
    }

    // ---------- metrics ----------
    #[cfg(feature = "metrics")]
    {
        use subms_block_cache::MetricsCache;
        h.add_meta("subms.workload.feature", "metrics");
        let mut c: MetricsCache<u32, u64> = MetricsCache::with_capacity(ENTRIES);
        for k in 0..ENTRIES as u32 {
            c.put(k, k as u64);
        }
        bench_get_hit(&mut h, "metrics_get_hit", |key| c.get(&key).is_some());
        bench_put(&mut h, "metrics_put", |key| {
            c.put(key, key as u64);
        });
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
