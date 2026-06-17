#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_reshape::recipe::ReshapeRecipe;

// Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
// sample is a FULL reshape - a long-to-wide pivot of a 4,096-row frame into a
// 256x16 grid keyed on a STRING category, or a wide-to-long melt of a 4,096-row
// frame into ROWS*4 long rows with a Str variable column. This is the
// analytical front, not the tick loop. The TYPICAL (p50) whole-frame reshape is
// sub-ms here, but the tail is allocation-bound (the bucket map + the
// materialised output columns), so we deliberately do NOT assert a sub-ms p99.
// The guard below is a generous "does not stall pathologically" bound the p99
// clears with margin; the honest number to read is throughput, captured in
// perf/rust.json. Kept symmetric with the Java sibling, whose tail is GC-bound.
const GUARD_NS: u64 = 40_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 5_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&ReshapeRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "pivot",
                p99_ns_max: GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "melt",
                p99_ns_max: GUARD_NS,
            },
        ],
    )
    .unwrap();
}
