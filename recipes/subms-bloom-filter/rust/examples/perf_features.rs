//! Feature classification bench. For each opt-in feature the harness benches its
//! representative op across a size sweep, lets `subms` DECIDE the latency
//! category (hot-path / structural / auxiliary), and merge-writes the decision +
//! measured `p99ByStage` into the recipe's per-language manifest
//! `.subms/features/rust.json` - preserving any other fields already there.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness counting scalable partitioned serde"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsPerfHarness, classify_feature, summarize};
#[allow(unused_imports)]
use subms_bloom_filter::BloomFilter;

// Three sizes so the classifier can read a p99-vs-N slope (flat -> hot-path,
// growing -> structural). Bloom's probes are O(k) in the hash count, not the set
// size, so they read flat -> hot-path.
const SIZES: [usize; 3] = [4_096, 32_768, 262_144];

fn keys() -> Vec<String> {
    (0..SIZES[SIZES.len() - 1])
        .map(|i| format!("key-{i}"))
        .collect()
}

/// p99 (ns) of `probe` over `size` keys against a filter built by `build`.
fn probe_p99<T>(
    size: usize,
    build: impl Fn() -> T,
    probe: impl Fn(&T, &str),
    ks: &[String],
) -> u64 {
    let f = build();
    let mut h = SubMsPerfHarness::new("bloom-feature", "rust");
    let st = h.stage("probe", size);
    for k in &ks[..size] {
        st.time(|| probe(&f, k));
    }
    summarize(&h)
        .stages
        .iter()
        .find(|s| s.name == "probe")
        .map_or(0, |s| s.p99_ns)
}

/// p99 (ns) of a mutating `op` over `size` keys against a fresh filter.
fn mutate_p99<T>(
    size: usize,
    mut make: impl FnMut() -> T,
    mut op: impl FnMut(&mut T, &str),
    ks: &[String],
) -> u64 {
    let mut f = make();
    let mut h = SubMsPerfHarness::new("bloom-feature", "rust");
    let st = h.stage("op", size);
    for k in &ks[..size] {
        st.time(|| op(&mut f, k));
    }
    summarize(&h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| s.p99_ns)
}

fn main() -> io::Result<()> {
    let ks = keys();
    let canon = SIZES[SIZES.len() - 1];

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);

    // ---------- counting: adds remove() over 4-bit counters ----------
    #[cfg(feature = "counting")]
    {
        use subms_bloom_filter::CountingBloomFilter;
        let fill = |n: usize| {
            let mut c = CountingBloomFilter::new(n);
            for k in &ks[..n] {
                c.add(k);
            }
            c
        };
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                (
                    n,
                    probe_p99(n, || fill(n), |c, k| _ = c.might_contain(k), &ks),
                )
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);
        let mut p99 = BTreeMap::new();
        p99.insert("contains".to_string(), sweep.last().unwrap().1);
        p99.insert(
            "add".to_string(),
            mutate_p99(
                canon,
                || CountingBloomFilter::new(canon),
                |c, k| c.add(k),
                &ks,
            ),
        );
        p99.insert(
            "remove".to_string(),
            mutate_p99(canon, || fill(canon), |c, k| c.remove(k), &ks),
        );
        manifest.set_feature("counting", cat, &p99, &reason);
    }

    // ---------- scalable: layered add, hit walks layers ----------
    #[cfg(feature = "scalable")]
    {
        use subms_bloom_filter::ScalableBloomFilter;
        let fill = |n: usize| {
            let mut s = ScalableBloomFilter::new(1_000);
            for k in &ks[..n] {
                s.add(k);
            }
            s
        };
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                (
                    n,
                    probe_p99(n, || fill(n), |s, k| _ = s.might_contain(k), &ks),
                )
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);
        let mut p99 = BTreeMap::new();
        p99.insert("contains".to_string(), sweep.last().unwrap().1);
        p99.insert(
            "add".to_string(),
            mutate_p99(
                canon,
                || ScalableBloomFilter::new(1_000),
                |s, k| s.add(k),
                &ks,
            ),
        );
        manifest.set_feature("scalable", cat, &p99, &reason);
    }

    // ---------- partitioned: k independent slices ----------
    #[cfg(feature = "partitioned")]
    {
        use subms_bloom_filter::PartitionedBloomFilter;
        let fill = |n: usize| {
            let mut p = PartitionedBloomFilter::new(n);
            for k in &ks[..n] {
                p.add(k);
            }
            p
        };
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                (
                    n,
                    probe_p99(n, || fill(n), |p, k| _ = p.might_contain(k), &ks),
                )
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);
        let mut p99 = BTreeMap::new();
        p99.insert("contains".to_string(), sweep.last().unwrap().1);
        p99.insert(
            "add".to_string(),
            mutate_p99(
                canon,
                || PartitionedBloomFilter::new(canon),
                |p, k| p.add(k),
                &ks,
            ),
        );
        manifest.set_feature("partitioned", cat, &p99, &reason);
    }

    // ---------- serde: derive only, no hot-path workload -> auxiliary ----------
    #[cfg(feature = "serde")]
    {
        let (cat, reason) = classify_feature(&[], None, None);
        manifest.set_feature("serde", cat, &BTreeMap::new(), &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
