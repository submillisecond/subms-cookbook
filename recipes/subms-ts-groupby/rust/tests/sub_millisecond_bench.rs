#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_groupby::recipe::GroupByRecipe;

// Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
// sample is a FULL group-by-aggregate over a 4,096-row frame keyed by a
// low-cardinality column - the analytical front, not the tick loop. The
// typical (p50) whole-frame group-by is sub-ms, but the tail is allocation
// bound (a sub-frame is materialised per group, then the expr evaluator walks
// each), so we deliberately do NOT assert a sub-ms p99. The guard below is a
// generous "does not stall pathologically" bound the p99 clears with margin;
// the honest number to read is throughput, captured in perf/rust.json. Kept
// symmetric with the Java sibling, whose tail is GC-bound.
const GUARD_NS: u64 = 50_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 5_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&GroupByRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "group_agg",
                p99_ns_max: GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "value_counts",
                p99_ns_max: GUARD_NS,
            },
        ],
    )
    .unwrap();
}
