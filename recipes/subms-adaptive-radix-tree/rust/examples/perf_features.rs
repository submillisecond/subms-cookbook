//! Per-feature bench: runs a 50k-entry byte-string workload against the
//! base `Art`, then each opt-in feature (`serialize`, `range-scan`,
//! `concurrent-reads`, `metrics`, `compaction`) when its Cargo feature
//! is enabled at compile time.
//!
//! The output JSON carries one stage block per feature operation - e.g.
//! `base_insert`, `serialize_write`, `range_scan_range` - so the cookbook
//! page fills in the per-feature p99 table from a single JSON file.
//!
//! Two stage shapes appear here:
//!
//! - **Per-key** ops (insert, lookup, delete, snapshot point-get) record
//!   one sample per key over the full 50k key universe.
//! - **Bulk** ops (whole-tree serialize, full range scan, snapshot
//!   capture, compaction pass) operate on the whole populated tree at
//!   once. A single bulk op is far above the per-key sub-ms budget, so
//!   it is repeated `BULK_REPS` times to build a distribution rather
//!   than a single sample.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness serialize range-scan concurrent-reads metrics compaction"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};
use subms_adaptive_radix_tree::Art;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;
const BULK_REPS: usize = 200;

fn key_at(i: usize) -> String {
    format!("key-{i}")
}

fn populate(tree: &mut Art<u64>) {
    for i in 0..ENTRIES {
        tree.insert(key_at(i).as_bytes(), i as u64);
    }
}

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("adaptive-radix-tree-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.input("bulk_reps", &BULK_REPS.to_string());
    h.add_meta("subms.recipe.slug", "subms-adaptive-radix-tree");
    h.add_meta("subms.recipe.category", "ordered-index");

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let mut tree: Art<u64> = Art::new();
        let stage = h
            .stage("base_insert", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let k = key_at(i);
            stage.time(|| {
                tree.insert(k.as_bytes(), i as u64);
            });
        }

        let stage = h
            .stage("base_lookup", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let k = key_at(i);
            stage.time(|| {
                let _ = tree.get(k.as_bytes());
            });
        }
    }

    // ---------- serialize ----------
    #[cfg(feature = "serialize")]
    {
        use subms_adaptive_radix_tree::{parse, write_to};
        h.add_meta("subms.workload.feature", "serialize");

        let mut tree: Art<u64> = Art::new();
        populate(&mut tree);

        let mut buf: Vec<u8> = Vec::new();
        let stage = h
            .stage("serialize_write", BULK_REPS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..BULK_REPS {
            buf.clear();
            stage.time(|| write_to(&tree, &mut buf).expect("write"));
        }

        let stage = h
            .stage("serialize_read", BULK_REPS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..BULK_REPS {
            stage.time(|| {
                let mut cursor = &buf[..];
                let restored: Art<u64> = parse(&mut cursor).expect("parse");
                std::hint::black_box(restored.len());
            });
        }
    }

    // ---------- range-scan ----------
    #[cfg(feature = "range-scan")]
    {
        use subms_adaptive_radix_tree::{Bound, range};
        h.add_meta("subms.workload.feature", "range-scan");

        let mut tree: Art<u64> = Art::new();
        populate(&mut tree);

        let stage = h
            .stage("range_scan_range", BULK_REPS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..BULK_REPS {
            stage.time(|| {
                let out = range(&tree, Bound::Unbounded, Bound::Unbounded);
                std::hint::black_box(out.len());
            });
        }
    }

    // ---------- concurrent-reads ----------
    #[cfg(feature = "concurrent-reads")]
    {
        use subms_adaptive_radix_tree::ArtSnapshot;
        h.add_meta("subms.workload.feature", "concurrent-reads");

        let mut tree: Art<u64> = Art::new();
        populate(&mut tree);

        let stage = h
            .stage("concurrent_reads_snapshot", BULK_REPS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..BULK_REPS {
            stage.time(|| {
                let snap = ArtSnapshot::from_tree(&tree);
                std::hint::black_box(snap.len());
            });
        }

        let snap = ArtSnapshot::from_tree(&tree);
        let stage = h
            .stage("concurrent_reads_get_on_snapshot", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let k = key_at(i);
            stage.time(|| {
                let _ = snap.get(k.as_bytes());
            });
        }
    }

    // ---------- metrics ----------
    #[cfg(feature = "metrics")]
    {
        use subms_adaptive_radix_tree::MeasuredArt;
        h.add_meta("subms.workload.feature", "metrics");

        let mut tree: MeasuredArt<u64> = MeasuredArt::new();
        let stage = h
            .stage("metrics_insert", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let k = key_at(i);
            stage.time(|| {
                tree.insert(k.as_bytes(), i as u64);
            });
        }

        let stage = h
            .stage("metrics_lookup", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let k = key_at(i);
            stage.time(|| {
                let _ = tree.get(k.as_bytes());
            });
        }
    }

    // ---------- compaction ----------
    #[cfg(feature = "compaction")]
    {
        use subms_adaptive_radix_tree::{compact, delete};
        h.add_meta("subms.workload.feature", "compaction");

        // Per-key delete over a freshly populated tree.
        let mut tree: Art<u64> = Art::new();
        populate(&mut tree);
        let stage = h
            .stage("compaction_delete", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES {
            let k = key_at(i);
            stage.time(|| {
                let _ = delete(&mut tree, k.as_bytes());
            });
        }

        // Bulk compact pass. Each rep rebuilds a populated tree, deletes
        // a fraction, then times the compaction sweep over what's left.
        let stage_capacity = BULK_REPS;
        let mut samples_taken = 0usize;
        let stage = h
            .stage("compaction_compact", stage_capacity)
            .with_kind(SubMsStageKind::BatchOp);
        while samples_taken < BULK_REPS {
            let mut dirty: Art<u64> = Art::new();
            populate(&mut dirty);
            for i in (0..ENTRIES).step_by(2) {
                delete(&mut dirty, key_at(i).as_bytes());
            }
            stage.time(|| {
                let changes = compact(&mut dirty);
                std::hint::black_box(changes);
            });
            samples_taken += 1;
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
