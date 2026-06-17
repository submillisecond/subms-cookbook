//! Per-feature bench: runs the same 50k-entry workload against the base
//! `CountMinSketch`, plus each opt-in feature (`heavy-hitters`,
//! `windowed`, `merge`) when its Cargo feature is enabled at compile
//! time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_add`, `heavy_hitters_add`, `windowed_tick`, `merge` - so the
//! cookbook page can fill in the per-feature p99 table without juggling
//! multiple JSON files.
//!
//! Demonstrates the `bench_keyed_op` / `bench_templated_op` boilerplate-
//! killers from the central `subms` crate: most stage blocks below are a
//! single line. The `merge` stage builds two full sketches outside the
//! timed region and times just the element-wise max pass.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness heavy-hitters windowed merge"

use std::io::{self, Write};

use subms::{
    SubMsPerfHarness, SubMsStageKind, bench_keyed_op, bench_templated_op, summarize,
    summary_to_json,
};
use subms_count_min_sketch::CountMinSketch;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;
const DEPTH: usize = 5;
const WIDTH: usize = 16384;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("count-min-sketch-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-count-min-sketch");
    h.add_meta("subms.recipe.category", "probabilistic");

    // ---------- base ----------
    h.add_meta("subms.workload.feature", "base");
    let mut cms = CountMinSketch::new(DEPTH, WIDTH);
    bench_keyed_op(&mut h, "base_add", ENTRIES, SEED, |key| cms.add(key));
    h.stage_mut("base_add")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);
    bench_keyed_op(&mut h, "base_estimate", ENTRIES, SEED, |key| {
        let _ = cms.estimate(key);
    });
    h.stage_mut("base_estimate")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);

    // ---------- heavy-hitters ----------
    #[cfg(feature = "heavy-hitters")]
    {
        use subms_count_min_sketch::HeavyHitters;
        h.add_meta("subms.workload.feature", "heavy-hitters");
        let mut hh = HeavyHitters::new(16, DEPTH, WIDTH);
        bench_keyed_op(&mut h, "heavy_hitters_add", ENTRIES, SEED, |key| {
            hh.add(key)
        });
        h.stage_mut("heavy_hitters_add")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_templated_op(
            &mut h,
            "heavy_hitters_top_k",
            ENTRIES,
            "ignored-{}",
            |_key| {
                let _ = hh.top();
            },
        );
        h.stage_mut("heavy_hitters_top_k")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- windowed ----------
    #[cfg(feature = "windowed")]
    {
        use subms_count_min_sketch::WindowedCountMinSketch;
        h.add_meta("subms.workload.feature", "windowed");
        let mut w = WindowedCountMinSketch::new(4, DEPTH, WIDTH);
        bench_keyed_op(&mut h, "windowed_add", ENTRIES, SEED, |key| w.add(key));
        h.stage_mut("windowed_add")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "windowed_estimate", ENTRIES, SEED, |key| {
            let _ = w.estimate(key);
        });
        h.stage_mut("windowed_estimate")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_templated_op(&mut h, "windowed_tick", ENTRIES, "ignored-{}", |_key| {
            w.tick();
        });
        h.stage_mut("windowed_tick")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- merge ----------
    #[cfg(feature = "merge")]
    {
        use subms_count_min_sketch::merge_into;
        h.add_meta("subms.workload.feature", "merge");
        // Build both sketches outside the timed region. The merge stage
        // times a single element-wise max pass over d*w cells.
        let mut dst = CountMinSketch::new(DEPTH, WIDTH);
        let mut src = CountMinSketch::new(DEPTH, WIDTH);
        bench_keyed_op(&mut h, "merge_build_dst", ENTRIES, SEED, |key| dst.add(key));
        h.stage_mut("merge_build_dst")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "merge_build_src", ENTRIES, SEED + 1, |key| {
            src.add(key)
        });
        h.stage_mut("merge_build_src")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        let stage = h.stage("merge", 1).with_kind(SubMsStageKind::BatchOp);
        stage.time(|| {
            merge_into(&mut dst, &src).expect("identical shape");
        });
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
