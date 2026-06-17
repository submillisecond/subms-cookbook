//! Per-feature bench: runs the same 50k-entry workload against the base
//! `HyperLogLog`, plus each opt-in feature (`sparse`, `union-intersect`)
//! when its Cargo feature is enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_add`, `sparse_add`, `union`, etc. - so the cookbook page can
//! fill in the per-feature p99 table without juggling multiple JSON
//! files.
//!
//! Demonstrates the `bench_keyed_op` boilerplate-killer from the
//! central `subms` crate. The set-op stages (`union` / `intersect`)
//! build two HLLs up front, then time repeated estimate calls via a
//! manual stage loop because they take no per-iteration key.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness sparse union-intersect"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, bench_keyed_op, summarize, summary_to_json};
use subms_hyperloglog::HyperLogLog;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;
const PRECISION: u32 = 14;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("hyperloglog-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-hyperloglog");
    h.add_meta("subms.recipe.category", "probabilistic");

    // ---------- base ----------
    h.add_meta("subms.workload.feature", "base");
    let mut hll = HyperLogLog::new(PRECISION);
    bench_keyed_op(&mut h, "base_add", ENTRIES, SEED, |key| hll.add(key));
    h.stage_mut("base_add")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);
    {
        let stage = h
            .stage("base_estimate", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| {
                let _ = hll.estimate();
            });
        }
    }

    // ---------- sparse ----------
    #[cfg(feature = "sparse")]
    {
        use subms_hyperloglog::SparseHyperLogLog;
        h.add_meta("subms.workload.feature", "sparse");
        let mut sparse = SparseHyperLogLog::new(PRECISION);
        bench_keyed_op(&mut h, "sparse_add", ENTRIES, SEED, |key| sparse.add(key));
        h.stage_mut("sparse_add")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        let stage = h
            .stage("sparse_estimate", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| {
                let _ = sparse.estimate();
            });
        }
    }

    // ---------- union-intersect ----------
    #[cfg(feature = "union-intersect")]
    {
        use subms_hyperloglog::{estimate_intersect, estimate_union};
        h.add_meta("subms.workload.feature", "union-intersect");

        let mut a = HyperLogLog::new(PRECISION);
        let mut b = HyperLogLog::new(PRECISION);
        for i in 0..ENTRIES {
            a.add(&format!("a-{i}"));
            // Half-overlap so the inclusion-exclusion path does real work.
            b.add(&format!("b-{}", i / 2));
        }

        {
            let stage = h.stage("union", ENTRIES).with_kind(SubMsStageKind::HotPath);
            for _ in 0..ENTRIES {
                stage.time(|| {
                    let _ = estimate_union(&a, &b);
                });
            }
        }
        {
            let stage = h
                .stage("intersect", ENTRIES)
                .with_kind(SubMsStageKind::HotPath);
            for _ in 0..ENTRIES {
                stage.time(|| {
                    let _ = estimate_intersect(&a, &b);
                });
            }
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
