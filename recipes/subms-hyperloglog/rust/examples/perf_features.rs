//! Feature classification bench. Each feature's representative op is swept
//! across three PRECISIONS, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! Precision is the sweep axis because it is the only size a HyperLogLog has:
//! `p` fixes the register array at `2^p`, and the cardinality being counted
//! changes nothing about the memory touched. `add` hashes and writes one
//! register regardless of `p`, so it should read flat; anything that folds the
//! whole register array - `estimate`, a union - should climb.
//!
//! The register count is capped at `2^18` by the constructor's clamp, so the
//! sweep cannot be pushed an octave higher the way a sketch's width can. That
//! caps the bulk sweep at a 64x span starting from a 4 KB array, where fixed
//! per-call cost is still a visible share of the measurement.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness sparse union-intersect"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_hyperloglog::HyperLogLog;

/// 4096 / 32768 / 262144 registers. 18 is the constructor's ceiling.
const PRECISIONS: [u32; 3] = [12, 15, 18];
const CANON_P: u32 = PRECISIONS[PRECISIONS.len() - 1];
/// Sparse-list lengths. `sparse` is swept over this rather than over precision;
/// see the sparse block for why precision is the wrong axis for that one.
const LIST_LENS: [usize; 3] = [4_096, 32_768, 262_144];
/// Keyed ops per measurement. Fixed across the sweep so a slope has one cause.
const OPS: usize = 20_000;
const MAX_KEYS: usize = LIST_LENS[LIST_LENS.len() - 1];
/// Samples per bulk op. A whole-structure call is far above the per-key budget,
/// so a distribution needs repeats rather than one shot. 256 is a FLOOR, not a
/// preference: the harness takes p99 as `sorted[floor(0.99 * n)]`, so at n <= 100
/// that index IS `n - 1` and the "p99" is the single worst sample. A structural
/// verdict then turns on whichever rep caught a page fault. 256 puts two samples
/// above the index and makes it a real percentile. Do not lower it.
const BULK_REPS: usize = 256;
const BULK_WARM: usize = 8;

fn keys() -> Vec<String> {
    (0..MAX_KEYS).map(|i| format!("key-{i}")).collect()
}

fn regs(p: u32) -> usize {
    1usize << p
}

fn keyed_p50<T>(f: &mut T, ks: &[String], mut op: impl FnMut(&mut T, &str)) -> u64 {
    keyed(f, ks, &mut op, true)
}

fn keyed_p99<T>(f: &mut T, ks: &[String], mut op: impl FnMut(&mut T, &str)) -> u64 {
    keyed(f, ks, &mut op, false)
}

fn keyed<T>(f: &mut T, ks: &[String], op: &mut impl FnMut(&mut T, &str), median: bool) -> u64 {
    let mut h = SubMsPerfHarness::new("hll-feature", "rust");
    let st = h.stage("op", ks.len());
    for k in ks {
        st.time(|| op(f, k));
    }
    stat(&h, median)
}

/// A whole-array op. `setup` runs OUTSIDE the timed region and the first
/// `BULK_WARM` reps are discarded: measured cold, a bulk op lands its
/// first-touch cost on whichever sweep point runs first, which reads as a curve
/// that FALLS with size - the opposite of the structural signal.
fn bulk<T>(mut setup: impl FnMut() -> T, mut op: impl FnMut(&mut T), median: bool) -> u64 {
    let mut input = setup();
    for _ in 0..BULK_WARM {
        op(&mut input);
    }
    let mut h = SubMsPerfHarness::new("hll-feature", "rust");
    let st = h.stage("op", BULK_REPS);
    for _ in 0..BULK_REPS {
        st.time(|| op(&mut input));
    }
    stat(&h, median)
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// Sweeps and PRINTS the curve, indexed by REGISTER COUNT rather than by
/// precision - the classifier reads the size column as a magnitude, and `p` is
/// its logarithm.
fn sweep(label: &str, mut at: impl FnMut(u32) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = PRECISIONS.iter().map(|&p| (regs(p), at(p))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

/// Sweeps over an explicit size column rather than over precision.
fn sweep_sizes(label: &str, sizes: &[usize], mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = sizes.iter().map(|&n| (n, at(n))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

fn main() -> io::Result<()> {
    let ks = keys();

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

    // The baseline is base `add`, the per-op path. NOT base `estimate`: that
    // folds all 2^p registers, so classifying a per-key feature against it would
    // let anything look free.
    let mut base = HyperLogLog::new(CANON_P);
    let base_p50 = keyed_p50(&mut base, &ks[..OPS], |h, k| h.add(k));
    eprintln!("base add p50: {base_p50}ns");

    // ---------- sparse: a linear entry list until it earns the dense array ----------
    #[cfg(feature = "sparse")]
    {
        use subms_hyperloglog::SparseHyperLogLog;
        // Swept over SPARSE LIST LENGTH, not over precision. `add` linear-probes
        // the list, so length is the cost driver; precision only sets it
        // indirectly through the `m/4` promotion threshold, and swept that way
        // the curve is a step rather than a slope. At p=12 and p=15 the
        // structure promotes early, so BOTH low points measure the dense floor
        // (100ns) rather than a small sparse probe, and at p=18 the list is
        // capped by the key count instead of by the threshold. The resulting
        // ratio landed either side of the classifier's guard - 40x in Rust,
        // 23x in Java - which is a measurement artefact, not a real disagreement.
        //
        // `with_threshold` exists for exactly this: pin promotion out of reach
        // and add n keys, and the swept axis IS the list length.
        //
        // The list is built to length n OUTSIDE the timed region, and the timed
        // ops are re-adds of keys already in it - a fixed OPS of them at every
        // size, so the op count is constant and the scan length is the only
        // thing varying. Re-adding rather than adding fresh keys keeps the list
        // from growing under measurement.
        let sw = sweep_sizes("sparse/add(list-len)", &LIST_LENS, |n| {
            let mut s = SparseHyperLogLog::with_threshold(CANON_P, n + 1);
            for k in &ks[..n] {
                s.add(k);
            }
            let mut h = SubMsPerfHarness::new("hll-feature", "rust");
            let st = h.stage("op", OPS);
            for i in 0..OPS {
                let k = &ks[(i * 7919) % n];
                st.time(|| s.add(k));
            }
            stat(&h, true)
        });
        // PINNED structural when the ratio test cannot carry it. `add`
        // linear-probes the sparse list, so it is O(entries) from the source and
        // the sweep above is monotonic and strongly rising. What it is not is
        // 32x: a long scan runs ~0.34 ns/element against ~0.93 for a short one,
        // so a true O(n) op measures ~23x over a 64x span and falls under the
        // classifier's 0.5 guard. Publishing that as hot-path would tell a
        // reader the probe is free at high precision. It is not, and the pin
        // says a human decided rather than dressing the decision as measured.
        let (cat, reason) = classify_feature(
            &sw,
            Some(base_p50),
            Some(subms::SubMsFeatureCategory::Structural),
        );

        let mut s = SparseHyperLogLog::new(CANON_P);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "add".to_string(),
            keyed_p99(&mut s, &ks[..OPS], |x, k| x.add(k)),
        );
        p99.insert(
            "estimate".to_string(),
            bulk(
                || {
                    let mut x = SparseHyperLogLog::new(CANON_P);
                    for k in &ks[..OPS] {
                        x.add(k);
                    }
                    x
                },
                |x| _ = x.estimate(),
                false,
            ),
        );
        manifest.set_feature("sparse", cat, &p99, &reason);
    }

    // ---------- union-intersect: pairwise folds over both register arrays ----------
    #[cfg(feature = "union-intersect")]
    {
        use subms_hyperloglog::{estimate_intersect, estimate_union};
        // Both HLLs are built by `setup`, outside the timed region. A union is a
        // pure read of two register arrays, so repeating it does identical work.
        // Filled with `m` keys, not a fixed count. OCCUPANCY has to be held
        // constant or it, not size, is what the sweep measures: `estimate` costs
        // `2f64.powi(-r)` per register and `powi(0)` takes a fast path, so a
        // fixed key set against a growing array leaves 92% of registers zero at
        // p=18 against 0% at p=12. That reads as a per-register cost falling
        // with size, and it compressed a triple-O(m) op to 26x over 64x.
        let build = |p: u32| {
            let n = regs(p);
            let mut a = HyperLogLog::new(p);
            let mut b = HyperLogLog::new(p);
            for (i, k) in ks[..n].iter().enumerate() {
                a.add(k);
                if i % 2 == 0 {
                    b.add(k);
                }
            }
            (a, b)
        };
        let sw = sweep("union-intersect/estimate_union", |p| {
            bulk(
                || build(p),
                |(a, b)| _ = estimate_union(a, b).expect("same precision"),
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "union".to_string(),
            bulk(
                || build(CANON_P),
                |(a, b)| _ = estimate_union(a, b).expect("same precision"),
                false,
            ),
        );
        p99.insert(
            "intersect".to_string(),
            bulk(
                || build(CANON_P),
                |(a, b)| _ = estimate_intersect(a, b).expect("same precision"),
                false,
            ),
        );
        manifest.set_feature("union-intersect", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
