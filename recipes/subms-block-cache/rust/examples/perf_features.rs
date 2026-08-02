//! Per-feature bench: sweeps each opt-in feature (`arc`, `tinylfu`, `weighted`,
//! `concurrent-shards`, `metrics`) across three cache capacities, lets
//! `classify_feature` DECIDE the category from the shape of that sweep, and
//! merge-writes the decision into `../.subms/features/rust.json`.
//!
//! A cache's "size" is its capacity, so the sweep fills to N and times the
//! lookup path there. A per-op cost that holds steady as N grows is `hot-path`;
//! one that climbs with N is `structural`. That is the claim worth measuring for
//! a cache: an eviction policy that quietly walks the resident set does not stay
//! sub-millisecond as the cache grows, and only a sweep catches it.
//!
//! The sweep classifies on p50. p99 over a few dozen samples is just the worst
//! one, and a single scheduler slice is large enough to swamp the size signal the
//! sweep is reading. The p99 still goes into the manifest for the stage table.
//!
//! This replaces the previous shape, which ran every variant at ONE capacity and
//! ASSERTED hot-path via `SubMsStageKind::HotPath`. An asserted category is an
//! opinion the bench cannot contradict; a sweep measures it, and can disagree.
//!
//! `clock-sweep` is not a feature here: it IS the base cache, so its lookup is
//! the baseline every feature is classified against.
//!
//! These p99 figures describe THIS machine. They are published only when the
//! manifest is stamped `p99_source: fleet`; a local run leaves the category,
//! which is machine independent, and no published number.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness arc tinylfu weighted concurrent-shards metrics"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{
    SubMsFeatureManifest, SubMsLcg, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize,
};

/// Cache capacities the sweep walks.
const SIZES: [usize; 3] = [4_096, 32_768, 262_144];
const SEED: u64 = 0;

fn stage_stats(h: &SubMsPerfHarness, name: &str) -> (u64, u64) {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == name)
        .map_or((0, 0), |s| (s.p50_ns, s.p99_ns))
}

/// (p50, p99) in ns of `n` reads of keys drawn from the resident set `0..n`.
fn get_hit(n: usize, mut get: impl FnMut(u32) -> bool) -> (u64, u64) {
    let mut rng = SubMsLcg::new(SEED);
    let mut h = SubMsPerfHarness::new("block-cache-feature", "rust");
    {
        let st = h.stage("op", n);
        for _ in 0..n {
            let key = rng.next_u32() % (n as u32);
            st.time(|| {
                let _ = get(key);
            });
        }
    }
    stage_stats(&h, "op")
}

/// (p50, p99) in ns of `n` inserts of FRESH keys against a full cache, so every
/// put drives an eviction - the path a capacity claim actually rests on.
fn put_evicting(n: usize, mut put: impl FnMut(u32)) -> (u64, u64) {
    let base = n as u32;
    let mut h = SubMsPerfHarness::new("block-cache-feature", "rust");
    {
        let st = h.stage("op", n);
        for i in 0..n {
            let key = base + i as u32;
            st.time(|| put(key));
        }
    }
    stage_stats(&h, "op")
}

fn main() -> io::Result<()> {
    let canon = SIZES[SIZES.len() - 1];

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

    // ---------- base (clock-sweep): the baseline, not a feature ----------
    // Every feature is classified against this. A variant whose lookup lands
    // within a whisker of the base is a capability, not a latency change, and
    // classify_feature says so rather than calling it hot-path by default.
    // The baseline is a p50, because the sweep values are p50s. Handing
    // classify_feature a base p99 against p50 sweep points compares two
    // different statistics: the p50 sits below the p99 almost by construction,
    // so every feature reads as "within 10% of base" and lands auxiliary.
    let base_p50 = {
        use subms_block_cache::BlockCache;
        let mut c: BlockCache<u32, u64> = BlockCache::with_capacity(canon);
        for k in 0..canon as u32 {
            c.put(k, k as u64);
        }
        let (p50, _) = get_hit(canon, |key| c.get(&key).is_some());
        p50
    };

    // ---------- arc: adaptive replacement, recency + frequency lists ----------
    #[cfg(feature = "arc")]
    {
        use subms_block_cache::ArcCache;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut c: ArcCache<u32, u64> = ArcCache::with_capacity(n);
                for k in 0..n as u32 {
                    c.put(k, k as u64);
                }
                let (p50, _) = get_hit(n, |key| c.get(&key).is_some());
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut c: ArcCache<u32, u64> = ArcCache::with_capacity(canon);
        for k in 0..canon as u32 {
            c.put(k, k as u64);
        }
        let (_, get99) = get_hit(canon, |key| c.get(&key).is_some());
        let (_, put99) = put_evicting(canon, |key| {
            c.put(key, key as u64);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("get_hit".to_string(), get99);
        p99.insert("put".to_string(), put99);
        manifest.set_feature("arc", cat, &p99, &reason);
    }

    // ---------- tinylfu: frequency-sketch admission ----------
    #[cfg(feature = "tinylfu")]
    {
        use subms_block_cache::TinyLfuCache;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut c: TinyLfuCache<u32, u64> = TinyLfuCache::with_capacity(n);
                for k in 0..n as u32 {
                    c.put(k, k as u64);
                }
                let (p50, _) = get_hit(n, |key| c.get(&key).is_some());
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut c: TinyLfuCache<u32, u64> = TinyLfuCache::with_capacity(canon);
        for k in 0..canon as u32 {
            c.put(k, k as u64);
        }
        let (_, get99) = get_hit(canon, |key| c.get(&key).is_some());
        let (_, put99) = put_evicting(canon, |key| {
            c.put(key, key as u64);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("get_hit".to_string(), get99);
        p99.insert("put".to_string(), put99);
        manifest.set_feature("tinylfu", cat, &p99, &reason);
    }

    // ---------- weighted: a byte budget rather than a slot count ----------
    #[cfg(feature = "weighted")]
    {
        use subms_block_cache::WeightedCache;
        // 1 byte per entry so capacity_bytes == slot capacity; eviction behaves
        // like the base cache, which isolates the weight bookkeeping itself.
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut c: WeightedCache<u32, u64> =
                    WeightedCache::with_capacity_bytes(n, |_v: &u64| 1);
                for k in 0..n as u32 {
                    c.put(k, k as u64);
                }
                let (p50, _) = get_hit(n, |key| c.get(&key).is_some());
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut c: WeightedCache<u32, u64> =
            WeightedCache::with_capacity_bytes(canon, |_v: &u64| 1);
        for k in 0..canon as u32 {
            c.put(k, k as u64);
        }
        let (_, get99) = get_hit(canon, |key| c.get(&key).is_some());
        let (_, put99) = put_evicting(canon, |key| {
            let _ = c.put(key, key as u64);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("get_hit".to_string(), get99);
        p99.insert("put".to_string(), put99);
        manifest.set_feature("weighted", cat, &p99, &reason);
    }

    // ---------- concurrent-shards: measured single-threaded ----------
    // Uncontended on purpose. This isolates the sharding INDIRECTION from the
    // contention it exists to relieve; a multi-threaded number here would say
    // more about the thread count than about the feature.
    #[cfg(feature = "concurrent-shards")]
    {
        use subms_block_cache::ShardedCache;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let c: ShardedCache<u32, u64> = ShardedCache::with_capacity(n, 16);
                for k in 0..n as u32 {
                    c.put(k, k as u64);
                }
                let (p50, _) = get_hit(n, |key| c.get(&key).is_some());
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let c: ShardedCache<u32, u64> = ShardedCache::with_capacity(canon, 16);
        for k in 0..canon as u32 {
            c.put(k, k as u64);
        }
        let (_, get99) = get_hit(canon, |key| c.get(&key).is_some());
        let (_, put99) = put_evicting(canon, |key| {
            c.put(key, key as u64);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("get_hit".to_string(), get99);
        p99.insert("put".to_string(), put99);
        manifest.set_feature("concurrent-shards", cat, &p99, &reason);
    }

    // ---------- metrics: hit/miss counters on the lookup path ----------
    #[cfg(feature = "metrics")]
    {
        use subms_block_cache::MetricsCache;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut c: MetricsCache<u32, u64> = MetricsCache::with_capacity(n);
                for k in 0..n as u32 {
                    c.put(k, k as u64);
                }
                let (p50, _) = get_hit(n, |key| c.get(&key).is_some());
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut c: MetricsCache<u32, u64> = MetricsCache::with_capacity(canon);
        for k in 0..canon as u32 {
            c.put(k, k as u64);
        }
        let (_, get99) = get_hit(canon, |key| c.get(&key).is_some());
        let (_, put99) = put_evicting(canon, |key| {
            c.put(key, key as u64);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("get_hit".to_string(), get99);
        p99.insert("put".to_string(), put99);
        manifest.set_feature("metrics", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
