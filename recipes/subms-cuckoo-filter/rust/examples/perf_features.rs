//! Per-feature bench: runs the same 50k-entry workload against the base
//! `CuckooFilter`, plus each opt-in feature (`variable-fingerprint`,
//! `dynamic`, `concurrent-reads`, `compressed-buckets`) when its Cargo
//! feature is enabled at compile time.
//!
//! The output JSON has one stage block per feature variant (e.g.
//! `base_insert`, `variable_fingerprint_insert`, `dynamic_lookup`),
//! so the cookbook page can fill in the per-feature p99 table from a
//! single JSON file.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness variable-fingerprint dynamic concurrent-reads compressed-buckets"

use std::io::{self, Write};

#[allow(unused_imports)]
use subms::SubMsLcg;
use subms::{SubMsPerfHarness, SubMsStageKind, bench_keyed_op, summarize, summary_to_json};
use subms_cuckoo_filter::CuckooFilter;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("cuckoo-filter-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-cuckoo-filter");
    h.add_meta("subms.recipe.category", "probabilistic");

    // ---------- base ----------
    h.add_meta("subms.workload.feature", "base");
    let mut cf = CuckooFilter::with_capacity(ENTRIES);
    bench_keyed_op(&mut h, "base_insert", ENTRIES, SEED, |key| {
        cf.insert(key);
    });
    h.stage_mut("base_insert")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);
    bench_keyed_op(&mut h, "base_lookup", ENTRIES, SEED, |key| {
        let _ = cf.contains(key);
    });
    h.stage_mut("base_lookup")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);
    bench_keyed_op(&mut h, "base_delete", ENTRIES, SEED, |key| {
        cf.delete(key);
    });
    h.stage_mut("base_delete")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);

    // ---------- variable-fingerprint ----------
    #[cfg(feature = "variable-fingerprint")]
    {
        use subms_cuckoo_filter::{FingerprintWidth, VariableFpCuckooFilter};
        h.add_meta("subms.workload.feature", "variable-fingerprint");
        // Sixteen-bit fingerprint: the widest option, the headline
        // memory-for-FPR tradeoff this feature exists to expose.
        let mut vf = VariableFpCuckooFilter::new(ENTRIES, FingerprintWidth::Sixteen);
        bench_keyed_op(
            &mut h,
            "variable_fingerprint_insert",
            ENTRIES,
            SEED,
            |key| {
                vf.insert(key);
            },
        );
        h.stage_mut("variable_fingerprint_insert")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(
            &mut h,
            "variable_fingerprint_lookup",
            ENTRIES,
            SEED,
            |key| {
                let _ = vf.contains(key);
            },
        );
        h.stage_mut("variable_fingerprint_lookup")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(
            &mut h,
            "variable_fingerprint_delete",
            ENTRIES,
            SEED,
            |key| {
                vf.delete(key);
            },
        );
        h.stage_mut("variable_fingerprint_delete")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- dynamic ----------
    #[cfg(feature = "dynamic")]
    {
        use subms_cuckoo_filter::DynamicCuckooFilter;
        h.add_meta("subms.workload.feature", "dynamic");
        let mut dy = DynamicCuckooFilter::new(ENTRIES);
        bench_keyed_op(&mut h, "dynamic_insert", ENTRIES, SEED, |key| {
            dy.insert(key);
        });
        h.stage_mut("dynamic_insert")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "dynamic_lookup", ENTRIES, SEED, |key| {
            let _ = dy.contains(key);
        });
        h.stage_mut("dynamic_lookup")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "dynamic_delete", ENTRIES, SEED, |key| {
            dy.delete(key);
        });
        h.stage_mut("dynamic_delete")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- concurrent-reads ----------
    #[cfg(feature = "concurrent-reads")]
    {
        use subms_cuckoo_filter::CuckooSnapshot;
        h.add_meta("subms.workload.feature", "concurrent-reads");
        // Populate the source filter untimed (same key universe as every
        // other stage, via the shared LCG), then time the one-shot
        // snapshot capture - a single O(N) bucket copy - followed by
        // per-key lookups against the frozen snapshot.
        let mut src = CuckooFilter::with_capacity(ENTRIES);
        let mut rng = SubMsLcg::new(SEED);
        for _ in 0..ENTRIES {
            src.insert(&format!("k{}", rng.next_u32()));
        }
        let snap = {
            let stage = h.stage("snapshot", 1).with_kind(SubMsStageKind::BatchOp);
            stage.time(|| CuckooSnapshot::capture(&src))
        };
        bench_keyed_op(&mut h, "lookup_on_snapshot", ENTRIES, SEED, |key| {
            let _ = snap.contains(key);
        });
        h.stage_mut("lookup_on_snapshot")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- compressed-buckets ----------
    #[cfg(feature = "compressed-buckets")]
    {
        use subms_cuckoo_filter::CompressedCuckooFilter;
        h.add_meta("subms.workload.feature", "compressed-buckets");
        let mut cb = CompressedCuckooFilter::with_capacity(ENTRIES);
        bench_keyed_op(&mut h, "compressed_buckets_insert", ENTRIES, SEED, |key| {
            cb.insert(key);
        });
        h.stage_mut("compressed_buckets_insert")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "compressed_buckets_lookup", ENTRIES, SEED, |key| {
            let _ = cb.contains(key);
        });
        h.stage_mut("compressed_buckets_lookup")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "compressed_buckets_delete", ENTRIES, SEED, |key| {
            cb.delete(key);
        });
        h.stage_mut("compressed_buckets_delete")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
