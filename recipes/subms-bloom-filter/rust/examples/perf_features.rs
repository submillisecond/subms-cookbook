//! Per-feature bench: runs the same 50k-entry workload against the base
//! `BloomFilter`, plus each opt-in feature (`counting`, `scalable`,
//! `partitioned`) when its Cargo feature is enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_add`, `counting_add`, `scalable_add`, etc. - so the cookbook
//! page can fill in the per-feature p99 table without juggling
//! multiple JSON files.
//!
//! Demonstrates the `bench_keyed_op` / `bench_templated_op` boilerplate-
//! killers from the central `subms` crate: every stage block below is a
//! single line instead of the 6-line "init rng + stage + for-loop +
//! format key + stage.time" pattern that used to live in every recipe.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness counting scalable partitioned"

use std::io::{self, Write};

use subms::{
    SubMsPerfHarness, SubMsStageKind, bench_keyed_op, bench_templated_op, summarize,
    summary_to_json,
};
use subms_bloom_filter::BloomFilter;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("bloom-filter-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-bloom-filter");
    h.add_meta("subms.recipe.category", "probabilistic");

    // ---------- base ----------
    h.add_meta("subms.workload.feature", "base");
    let mut bf = BloomFilter::new(ENTRIES);
    bench_keyed_op(&mut h, "base_add", ENTRIES, SEED, |key| bf.add(key));
    h.stage_mut("base_add")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);
    bench_keyed_op(&mut h, "base_hit", ENTRIES, SEED, |key| {
        let _ = bf.might_contain(key);
    });
    h.stage_mut("base_hit")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);
    bench_templated_op(&mut h, "base_miss", ENTRIES, "miss-{}", |key| {
        let _ = bf.might_contain(key);
    });
    h.stage_mut("base_miss")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);

    // ---------- counting ----------
    #[cfg(feature = "counting")]
    {
        use subms_bloom_filter::CountingBloomFilter;
        h.add_meta("subms.workload.feature", "counting");
        let mut cb = CountingBloomFilter::new(ENTRIES);
        bench_keyed_op(&mut h, "counting_add", ENTRIES, SEED, |key| cb.add(key));
        h.stage_mut("counting_add")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "counting_hit", ENTRIES, SEED, |key| {
            let _ = cb.might_contain(key);
        });
        h.stage_mut("counting_hit")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "counting_remove", ENTRIES, SEED, |key| {
            cb.remove(key)
        });
        h.stage_mut("counting_remove")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- scalable ----------
    #[cfg(feature = "scalable")]
    {
        use subms_bloom_filter::ScalableBloomFilter;
        h.add_meta("subms.workload.feature", "scalable");
        let mut sb = ScalableBloomFilter::new(1_000);
        bench_keyed_op(&mut h, "scalable_add", ENTRIES, SEED, |key| sb.add(key));
        h.stage_mut("scalable_add")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "scalable_hit", ENTRIES, SEED, |key| {
            let _ = sb.might_contain(key);
        });
        h.stage_mut("scalable_hit")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- partitioned ----------
    #[cfg(feature = "partitioned")]
    {
        use subms_bloom_filter::PartitionedBloomFilter;
        h.add_meta("subms.workload.feature", "partitioned");
        let mut pb = PartitionedBloomFilter::new(ENTRIES);
        bench_keyed_op(&mut h, "partitioned_add", ENTRIES, SEED, |key| pb.add(key));
        h.stage_mut("partitioned_add")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_keyed_op(&mut h, "partitioned_hit", ENTRIES, SEED, |key| {
            let _ = pb.might_contain(key);
        });
        h.stage_mut("partitioned_hit")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
        bench_templated_op(&mut h, "partitioned_miss", ENTRIES, "miss-{}", |key| {
            let _ = pb.might_contain(key);
        });
        h.stage_mut("partitioned_miss")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
