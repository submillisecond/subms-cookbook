//! Per-feature bench: sweeps each opt-in feature (`serialize`, `range-scan`,
//! `concurrent-reads`, `metrics`, `compaction`) across three tree sizes, lets
//! `classify_feature` DECIDE the category from the shape of that sweep, and
//! merge-writes the decision into `../.subms/features/rust.json` - preserving
//! any field already there.
//!
//! The category is measured, not asserted. A p99 that stays flat as the tree
//! grows is `hot-path`; one that scales with size is `structural` and sits
//! outside the per-op sub-ms claim. Each feature is swept on the operation a
//! caller actually repeats: the whole-tree ops (serialize, range, compact) on
//! that bulk call, the per-key ops (snapshot reads, measured lookups) on the
//! key operation.
//!
//! The p99 figures written here describe THIS machine. They are published only
//! when the manifest is stamped `p99_source: fleet`; a local run leaves the
//! category, which is machine independent, and no published number.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness serialize range-scan concurrent-reads metrics compaction"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{
    SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize,
};
use subms_adaptive_radix_tree::Art;

/// Sizes the sweep walks. A 64x span separates a flat per-op cost from one that
/// tracks the structure.
const SIZES: [usize; 3] = [4_096, 32_768, 262_144];
/// Samples per bulk op. A whole-structure call is far above the per-key budget,
/// so a distribution needs repeats rather than one shot. 256 is a FLOOR, not a
/// preference: the harness takes p99 as `sorted[floor(0.99 * n)]`, so at n <= 100
/// that index IS `n - 1` and the "p99" is the single worst sample. A structural
/// verdict then turns on whichever rep caught a page fault. 256 puts two samples
/// above the index and makes it a real percentile. Do not lower it.
const BULK_REPS: usize = 256;

fn key_at(i: usize) -> String {
    format!("key-{i}")
}

fn populate(n: usize) -> Art<u64> {
    let mut tree: Art<u64> = Art::new();
    for i in 0..n {
        tree.insert(key_at(i).as_bytes(), i as u64);
    }
    tree
}

fn stage_p99(h: &SubMsPerfHarness, name: &str) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == name)
        .map_or(0, |s| s.p99_ns)
}

/// p99 (ns) of a whole-structure op repeated `BULK_REPS` times.
fn bulk_p99(mut op: impl FnMut()) -> u64 {
    let mut h = SubMsPerfHarness::new("art-feature", "rust");
    {
        let st = h.stage("bulk", BULK_REPS);
        for _ in 0..BULK_REPS {
            st.time(&mut op);
        }
    }
    stage_p99(&h, "bulk")
}

/// p99 (ns) of a DESTRUCTIVE whole-tree op that needs a fresh input each rep.
///
/// `setup` builds the input and is NOT timed; only `op` is. Folding the setup
/// into the timed closure - the obvious way to get a fresh tree per rep - times
/// the build and the deletes as well as the op, so the figure scales with the
/// setup rather than with the thing being measured, and reads far higher than
/// the op costs.
fn bulk_p99_with<T>(mut setup: impl FnMut() -> T, mut op: impl FnMut(&mut T)) -> u64 {
    let mut h = SubMsPerfHarness::new("art-feature", "rust");
    {
        let st = h.stage("bulk", BULK_REPS);
        for _ in 0..BULK_REPS {
            let mut input = setup();
            st.time(|| op(&mut input));
        }
    }
    stage_p99(&h, "bulk")
}

/// p99 (ns) of a per-key op run once over every key in `0..n`.
fn keyed_p99(n: usize, mut op: impl FnMut(&[u8])) -> u64 {
    let mut h = SubMsPerfHarness::new("art-feature", "rust");
    {
        let st = h.stage("keyed", n);
        for i in 0..n {
            let k = key_at(i);
            st.time(|| op(k.as_bytes()));
        }
    }
    stage_p99(&h, "keyed")
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

    // ---------- serialize: whole-tree write + parse ----------
    #[cfg(feature = "serialize")]
    {
        use subms_adaptive_radix_tree::{parse, write_to};
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let tree = populate(n);
                let mut buf = Vec::new();
                (
                    n,
                    bulk_p99(|| {
                        buf.clear();
                        write_to(&tree, &mut buf).expect("write");
                    }),
                )
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let tree = populate(canon);
        let mut buf = Vec::new();
        write_to(&tree, &mut buf).expect("write");
        let read = bulk_p99(|| {
            let mut cursor = &buf[..];
            let restored: Art<u64> = parse(&mut cursor).expect("parse");
            std::hint::black_box(restored.len());
        });

        let mut p99 = BTreeMap::new();
        p99.insert("write".to_string(), sweep.last().unwrap().1);
        p99.insert("read".to_string(), read);
        manifest.set_feature("serialize", cat, &p99, &reason);
    }

    // ---------- range-scan: a full scan tracks the tree it walks ----------
    #[cfg(feature = "range-scan")]
    {
        use subms_adaptive_radix_tree::{Bound, range};
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let tree = populate(n);
                (
                    n,
                    bulk_p99(|| {
                        let out = range(&tree, Bound::Unbounded, Bound::Unbounded);
                        std::hint::black_box(out.len());
                    }),
                )
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);
        let mut p99 = BTreeMap::new();
        p99.insert("range".to_string(), sweep.last().unwrap().1);
        manifest.set_feature("range-scan", cat, &p99, &reason);
    }

    // ---------- concurrent-reads: the point is the READ off a frozen view ----------
    #[cfg(feature = "concurrent-reads")]
    {
        use subms_adaptive_radix_tree::ArtSnapshot;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let tree = populate(n);
                let snap = ArtSnapshot::from_tree(&tree);
                (n, keyed_p99(n, |k| _ = snap.get(k)))
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        // Taking the snapshot is O(n) and is NOT the swept op - recorded so the
        // page shows what establishing the frozen view costs.
        let tree = populate(canon);
        let snapshot = bulk_p99(|| {
            let snap = ArtSnapshot::from_tree(&tree);
            std::hint::black_box(snap.len());
        });

        let mut p99 = BTreeMap::new();
        p99.insert("get".to_string(), sweep.last().unwrap().1);
        p99.insert("snapshot".to_string(), snapshot);
        manifest.set_feature("concurrent-reads", cat, &p99, &reason);
    }

    // ---------- metrics: counters on the insert/lookup path ----------
    #[cfg(feature = "metrics")]
    {
        use subms_adaptive_radix_tree::MeasuredArt;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let mut tree: MeasuredArt<u64> = MeasuredArt::new();
                for i in 0..n {
                    tree.insert(key_at(i).as_bytes(), i as u64);
                }
                (n, keyed_p99(n, |k| _ = tree.get(k)))
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let mut fresh: MeasuredArt<u64> = MeasuredArt::new();
        let mut next = 0u64;
        let insert = keyed_p99(canon, |k| {
            fresh.insert(k, next);
            next += 1;
        });

        let mut p99 = BTreeMap::new();
        p99.insert("lookup".to_string(), sweep.last().unwrap().1);
        p99.insert("insert".to_string(), insert);
        manifest.set_feature("metrics", cat, &p99, &reason);
    }

    // ---------- compaction: a sweep over what the deletes left behind ----------
    #[cfg(feature = "compaction")]
    {
        use subms_adaptive_radix_tree::{compact, delete};
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                // Build + delete are setup, not the measurement. Only compact()
                // is inside the timed region.
                let p99 = bulk_p99_with(
                    || {
                        let mut dirty = populate(n);
                        for i in (0..n).step_by(2) {
                            delete(&mut dirty, key_at(i).as_bytes());
                        }
                        dirty
                    },
                    |dirty| {
                        std::hint::black_box(compact(dirty));
                    },
                );
                (n, p99)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, None, None);

        let mut tree = populate(canon);
        let del = keyed_p99(canon, |k| _ = delete(&mut tree, k));

        let mut p99 = BTreeMap::new();
        p99.insert("compact".to_string(), sweep.last().unwrap().1);
        p99.insert("delete".to_string(), del);
        manifest.set_feature("compaction", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
