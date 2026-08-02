//! Per-feature bench: sweeps each opt-in feature (`variable-fingerprint`,
//! `dynamic`, `concurrent-reads`, `compressed-buckets`) across three filter
//! sizes, lets `classify_feature` DECIDE the category from the shape of that
//! sweep, and merge-writes the decision into `../.subms/features/rust.json`.
//!
//! A filter's "size" is how many keys it is holding, so the sweep fills to N and
//! times the lookup path there. A per-op cost that holds steady as N grows is
//! `hot-path`; one that climbs with N is `structural`. For a cuckoo filter that
//! is the claim worth measuring: a variant whose lookup starts chasing longer
//! eviction chains as the table fills does not stay sub-millisecond, and only a
//! sweep catches it.
//!
//! `concurrent-reads` is the one feature that is genuinely two different things:
//! the SNAPSHOT is an O(N) bucket copy, and lookups against the frozen snapshot
//! are per-op. The sweep classifies on the snapshot, because that is the part
//! whose cost depends on size; the per-key lookup p99 still lands in the stage
//! table.
//!
//! The sweep classifies on p50, and the BASELINE is a p50 too. p99 over a few
//! dozen samples is just the worst one, and a single scheduler slice is large
//! enough to swamp the size signal. Mixing the two - a p50 sweep point against a
//! p99 baseline - compares different statistics, and the p50 sits under the p99
//! almost by construction, so every feature would read as a non-effect.
//!
//! This replaces the previous shape, which ran every variant at ONE size and
//! ASSERTED hot-path via `SubMsStageKind::HotPath`. An asserted category is an
//! opinion the bench cannot contradict; a sweep measures it, and can disagree.
//!
//! These p99 figures describe THIS machine. They are published only when the
//! manifest is stamped `p99_source: fleet`; a local run leaves the category,
//! which is machine independent for the SCALING verdict, and no published number.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness variable-fingerprint dynamic concurrent-reads compressed-buckets"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{
    SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize,
};
use subms_cuckoo_filter::CuckooFilter;

/// Key counts the sweep walks.
const SIZES: [usize; 3] = [4_096, 32_768, 262_144];
/// Timed repeats for the one-shot snapshot capture.
const SNAPSHOT_REPS: usize = 32;
/// UNTIMED captures before them. A whole-table copy pays first-touch on a fresh
/// heap region, and that cost lands entirely on the SMALLEST sweep point - which
/// is the denominator of the scaling ratio. Left in, it inflated 4096 to 7100ns
/// against 3000ns at 32768, a non-monotonic sweep whose min/max ratio read 2.3x
/// over a 64x size range, so an O(N) memcpy classified hot-path. Warmup has to
/// be discarded, not merely amortised over a few samples.
const SNAPSHOT_WARM: usize = 16;

fn keys(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("key-{i}")).collect()
}

fn stage_stats(h: &SubMsPerfHarness, name: &str) -> (u64, u64) {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == name)
        .map_or((0, 0), |s| (s.p50_ns, s.p99_ns))
}

/// (p50, p99) in ns of `op` run once per key.
fn keyed(ks: &[String], mut op: impl FnMut(&str)) -> (u64, u64) {
    let mut h = SubMsPerfHarness::new("cuckoo-feature", "rust");
    {
        let st = h.stage("op", ks.len());
        for k in ks {
            st.time(|| op(k));
        }
    }
    stage_stats(&h, "op")
}

fn main() -> io::Result<()> {
    let canon = SIZES[SIZES.len() - 1];
    let canon_keys = keys(canon);

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

    // ---------- base: the baseline, not a feature ----------
    // Every feature is classified against this. A variant whose lookup lands at
    // or under the base costs nothing on the hot path, and classify_feature says
    // so rather than calling it hot-path by default.
    let base_p50 = {
        let mut f = CuckooFilter::with_capacity(canon);
        for k in &canon_keys {
            f.insert(k);
        }
        let (p50, _) = keyed(&canon_keys, |k| {
            let _ = f.contains(k);
        });
        p50
    };

    // ---------- variable-fingerprint: wider tag, lower FPR ----------
    #[cfg(feature = "variable-fingerprint")]
    {
        use subms_cuckoo_filter::{FingerprintWidth, VariableFpCuckooFilter};
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let ks = keys(n);
                let mut f = VariableFpCuckooFilter::new(n, FingerprintWidth::Sixteen);
                for k in &ks {
                    f.insert(k);
                }
                let (p50, _) = keyed(&ks, |k| {
                    let _ = f.contains(k);
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut f = VariableFpCuckooFilter::new(canon, FingerprintWidth::Sixteen);
        let (_, insert99) = keyed(&canon_keys, |k| {
            f.insert(k);
        });
        let (_, lookup99) = keyed(&canon_keys, |k| {
            let _ = f.contains(k);
        });
        let (_, delete99) = keyed(&canon_keys, |k| {
            f.delete(k);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("insert".to_string(), insert99);
        p99.insert("lookup".to_string(), lookup99);
        p99.insert("delete".to_string(), delete99);
        manifest.set_feature("variable-fingerprint", cat, &p99, &reason);
    }

    // ---------- dynamic: grows rather than refusing at load factor ----------
    #[cfg(feature = "dynamic")]
    {
        use subms_cuckoo_filter::DynamicCuckooFilter;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let ks = keys(n);
                let mut f = DynamicCuckooFilter::new(n);
                for k in &ks {
                    f.insert(k);
                }
                let (p50, _) = keyed(&ks, |k| {
                    let _ = f.contains(k);
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut f = DynamicCuckooFilter::new(canon);
        let (_, insert99) = keyed(&canon_keys, |k| {
            f.insert(k);
        });
        let (_, lookup99) = keyed(&canon_keys, |k| {
            let _ = f.contains(k);
        });
        let (_, delete99) = keyed(&canon_keys, |k| {
            f.delete(k);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("insert".to_string(), insert99);
        p99.insert("lookup".to_string(), lookup99);
        p99.insert("delete".to_string(), delete99);
        manifest.set_feature("dynamic", cat, &p99, &reason);
    }

    // ---------- concurrent-reads: a frozen snapshot readers share ----------
    // Classified on the SNAPSHOT, not the lookup. The snapshot is a whole-table
    // copy whose cost is the thing that scales; the lookups against it are
    // per-op and would classify the same as any other read.
    #[cfg(feature = "concurrent-reads")]
    {
        use subms_cuckoo_filter::CuckooSnapshot;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let ks = keys(n);
                let mut src = CuckooFilter::with_capacity(n);
                for k in &ks {
                    src.insert(k);
                }
                // Several samples, not one. A single timed capture at the
                // SMALLEST size absorbs the first-touch allocation cost, which
                // inflates the low end of the sweep and flattens the very ratio
                // the scaling test reads - a whole-table copy then classifies
                // hot-path, which is exactly backwards.
                for _ in 0..SNAPSHOT_WARM {
                    let _ = CuckooSnapshot::capture(&src);
                }
                let mut h = SubMsPerfHarness::new("cuckoo-feature", "rust");
                {
                    let st = h.stage("op", SNAPSHOT_REPS);
                    for _ in 0..SNAPSHOT_REPS {
                        st.time(|| {
                            let _ = CuckooSnapshot::capture(&src);
                        });
                    }
                }
                let (p50, _) = stage_stats(&h, "op");
                (n, p50)
            })
            .collect();
        // PINNED structural, not measured. `CuckooSnapshot::capture` is a
        // `to_vec()` of the whole bucket array - unambiguously O(N) from the
        // source - but this sweep cannot demonstrate it on a dev box: even with
        // warmup discarded the smallest size measures ~7us against ~3us at 8x
        // the size, a non-monotonic curve whose min/max ratio reads ~2x over a
        // 64x size range, so the scaling test calls it flat and an O(N) memcpy
        // classifies hot-path. Recording that would be a false claim about the
        // one op on this page that genuinely is not per-op, so the category is
        // pinned and `perfReason` says it was overridden rather than measured.
        // Revisit on a fleet capture, where the curve should separate.
        //
        // No base comparison either: a whole-table copy is not the same kind of
        // operation as a per-key lookup, so a delta against it means nothing.
        let (cat, reason) = classify_feature(
            &sweep,
            None,
            Some(subms::SubMsFeatureCategory::Structural),
        );

        let mut src = CuckooFilter::with_capacity(canon);
        for k in &canon_keys {
            src.insert(k);
        }
        for _ in 0..SNAPSHOT_WARM {
            let _ = CuckooSnapshot::capture(&src);
        }
        let mut h = SubMsPerfHarness::new("cuckoo-feature", "rust");
        let snap = {
            let st = h.stage("op", SNAPSHOT_REPS);
            for _ in 0..SNAPSHOT_REPS - 1 {
                st.time(|| {
                    let _ = CuckooSnapshot::capture(&src);
                });
            }
            st.time(|| CuckooSnapshot::capture(&src))
        };
        let (_, snap99) = stage_stats(&h, "op");
        let (_, lookup99) = keyed(&canon_keys, |k| {
            let _ = snap.contains(k);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("snapshot".to_string(), snap99);
        p99.insert("lookup_on_snapshot".to_string(), lookup99);
        manifest.set_feature("concurrent-reads", cat, &p99, &reason);
    }

    // ---------- compressed-buckets: tighter memory per bucket ----------
    #[cfg(feature = "compressed-buckets")]
    {
        use subms_cuckoo_filter::CompressedCuckooFilter;
        let sweep: Vec<(usize, u64)> = SIZES
            .iter()
            .map(|&n| {
                let ks = keys(n);
                let mut f = CompressedCuckooFilter::with_capacity(n);
                for k in &ks {
                    f.insert(k);
                }
                let (p50, _) = keyed(&ks, |k| {
                    let _ = f.contains(k);
                });
                (n, p50)
            })
            .collect();
        let (cat, reason) = classify_feature(&sweep, Some(base_p50), None);

        let mut f = CompressedCuckooFilter::with_capacity(canon);
        let (_, insert99) = keyed(&canon_keys, |k| {
            f.insert(k);
        });
        let (_, lookup99) = keyed(&canon_keys, |k| {
            let _ = f.contains(k);
        });
        let (_, delete99) = keyed(&canon_keys, |k| {
            f.delete(k);
        });
        let mut p99 = BTreeMap::new();
        p99.insert("insert".to_string(), insert99);
        p99.insert("lookup".to_string(), lookup99);
        p99.insert("delete".to_string(), delete99);
        manifest.set_feature("compressed-buckets", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
