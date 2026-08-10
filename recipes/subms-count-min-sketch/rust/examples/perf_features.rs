//! Feature classification bench. Each feature's representative op is swept
//! across three sketch WIDTHS, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! Width is the right sweep axis for a sketch. A count-min sketch is a fixed
//! `depth x width` table and a per-key op touches one cell per row, so a per-key
//! cost that holds steady as width grows is hot-path; one that climbs is walking
//! the whole table. `tick` and `merge` do exactly that, and only the sweep
//! separates them from `add`.
//!
//! The key COUNT is held constant across sweep points. Varying the table size
//! and the op count together would leave two explanations for any slope.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness heavy-hitters windowed merge"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_count_min_sketch::CountMinSketch;

const WIDTHS: [usize; 3] = [4_096, 32_768, 262_144];
/// Bulk ops sweep an octave higher. A whole-table op has a fixed per-call cost
/// that dominates at 4096 (a 80 KB clear runs 0.107 ns/cell against 0.051 ns/cell
/// at the top), which COMPRESSES the measured ratio: `tick` swept over the keyed
/// widths reads 30x over 64x and falls just under the classifier's 0.5 guard,
/// so a genuinely O(n) op classifies flat. Starting an octave up measures the
/// asymptote rather than the call overhead.
const BULK_WIDTHS: [usize; 3] = [32_768, 262_144, 2_097_152];
const CANON: usize = WIDTHS[WIDTHS.len() - 1];
const DEPTH: usize = 5;
/// Keyed ops per measurement. Fixed across the sweep - see the module note.
const OPS: usize = 20_000;
/// Samples per bulk op. A whole-structure call is far above the per-key budget,
/// so a distribution needs repeats rather than one shot. 256 is a FLOOR, not a
/// preference: the harness takes p99 as `sorted[floor(0.99 * n)]`, so at n <= 100
/// that index IS `n - 1` and the "p99" is the single worst sample. A structural
/// verdict then turns on whichever rep caught a page fault. 256 puts two samples
/// above the index and makes it a real percentile. Do not lower it.
const BULK_REPS: usize = 256;
const BULK_WARM: usize = 8;

fn keys() -> Vec<String> {
    (0..OPS).map(|i| format!("key-{i}")).collect()
}

/// p50 (ns) of `op` over every key. p50 rather than p99 because the sweep is
/// read as a slope, and a p99 over a few thousand samples moves on one outlier -
/// which swamps the size signal it is there to expose.
fn keyed_p50<T>(f: &mut T, ks: &[String], mut op: impl FnMut(&mut T, &str)) -> u64 {
    keyed(f, ks, &mut op, true)
}

fn keyed_p99<T>(f: &mut T, ks: &[String], mut op: impl FnMut(&mut T, &str)) -> u64 {
    keyed(f, ks, &mut op, false)
}

fn keyed<T>(f: &mut T, ks: &[String], op: &mut impl FnMut(&mut T, &str), median: bool) -> u64 {
    let mut h = SubMsPerfHarness::new("cms-feature", "rust");
    let st = h.stage("op", ks.len());
    for k in ks {
        st.time(|| op(f, k));
    }
    stat(&h, median)
}

/// A whole-table op. `setup` runs OUTSIDE the timed region and the first
/// `BULK_WARM` reps are discarded: measured cold, a bulk op lands its
/// first-touch cost on whichever sweep point runs first, which reads as a curve
/// that FALLS with size - the opposite of the structural signal, and just as
/// wrong.
fn bulk<T>(mut setup: impl FnMut() -> T, mut op: impl FnMut(&mut T), median: bool) -> u64 {
    let mut input = setup();
    for _ in 0..BULK_WARM {
        op(&mut input);
    }
    let mut h = SubMsPerfHarness::new("cms-feature", "rust");
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

/// Sweeps and PRINTS the curve. A non-monotonic or ratio-compressed sweep
/// classifies flat, and the only way to catch one is to look at the rows.
fn sweep(label: &str, widths: &[usize], mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = widths.iter().map(|&w| (w, at(w))).collect();
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

    // The baseline: a base-sketch estimate at the canonical width. A feature
    // landing at or under this costs nothing on the read path.
    let mut base = CountMinSketch::new(DEPTH, CANON);
    for k in &ks {
        base.add(k);
    }
    let base_p50 = keyed_p50(&mut base, &ks, |c, k| _ = c.estimate(k));
    eprintln!("base estimate p50: {base_p50}ns");

    // ---------- heavy-hitters: a top-K list kept alongside the counts ----------
    #[cfg(feature = "heavy-hitters")]
    {
        use subms_count_min_sketch::HeavyHitters;
        let sw = sweep("heavy-hitters/estimate", &WIDTHS, |w| {
            let mut hh = HeavyHitters::new(16, DEPTH, w);
            for k in &ks {
                hh.add(k);
            }
            keyed_p50(&mut hh, &ks, |h, k| _ = h.estimate(k))
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut hh = HeavyHitters::new(16, DEPTH, CANON);
        let mut p99 = BTreeMap::new();
        p99.insert("add".to_string(), keyed_p99(&mut hh, &ks, |h, k| h.add(k)));
        p99.insert(
            "estimate".to_string(),
            keyed_p99(&mut hh, &ks, |h, k| _ = h.estimate(k)),
        );
        p99.insert(
            "top_k".to_string(),
            keyed_p99(&mut hh, &ks, |h, _| _ = h.top()),
        );
        manifest.set_feature("heavy-hitters", cat, &p99, &reason);
    }

    // ---------- windowed: a ring of slices; tick clears the oldest ----------
    #[cfg(feature = "windowed")]
    {
        use subms_count_min_sketch::WindowedCountMinSketch;
        // Swept on `tick`, not on `add`. `add` is O(depth) like the base and
        // says nothing about the feature; `tick` is what a windowed sketch is
        // FOR, is O(depth*width), and is the cost a reader is deciding whether
        // to put on their hot path.
        let sw = sweep("windowed/tick", &BULK_WIDTHS, |w| {
            bulk(
                || WindowedCountMinSketch::new(4, DEPTH, w),
                |x| x.tick(),
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut w = WindowedCountMinSketch::new(4, DEPTH, CANON);
        let mut p99 = BTreeMap::new();
        p99.insert("add".to_string(), keyed_p99(&mut w, &ks, |x, k| x.add(k)));
        p99.insert(
            "estimate".to_string(),
            keyed_p99(&mut w, &ks, |x, k| _ = x.estimate(k)),
        );
        p99.insert(
            "tick".to_string(),
            bulk(
                || WindowedCountMinSketch::new(4, DEPTH, CANON),
                |x| x.tick(),
                false,
            ),
        );
        manifest.set_feature("windowed", cat, &p99, &reason);
    }

    // ---------- merge: element-wise sum over depth*width cells ----------
    #[cfg(feature = "merge")]
    {
        use subms_count_min_sketch::merge_into;
        // Both sketches come from `setup`, outside the timed region. Repeated
        // merges of the same pair walk the same cell count at the same cost -
        // a saturating add does not care what the cells hold - so the figure is
        // the merge and nothing else.
        let build = |w: usize| {
            let mut dst = CountMinSketch::new(DEPTH, w);
            let mut src = CountMinSketch::new(DEPTH, w);
            for k in &ks {
                dst.add(k);
                src.add(k);
            }
            (dst, src)
        };
        let sw = sweep("merge/merge_into", &BULK_WIDTHS, |w| {
            bulk(
                || build(w),
                |(d, s)| merge_into(d, s).expect("identical shape"),
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "merge".to_string(),
            bulk(
                || build(CANON),
                |(d, s)| merge_into(d, s).expect("identical shape"),
                false,
            ),
        );
        manifest.set_feature("merge", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
