#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_window::recipe::WindowRecipe;

// Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
// sample is a FULL window pass over a partitioned 4,096-row frame: a partition
// grouping, a per-partition sort/scan, and (for `over`) a per-partition
// aggregate. This is the analytical front, not the tick loop. The TYPICAL
// (p50) whole-frame pass is sub-ms, but the tail is allocation-bound (each
// pass materialises columns + per-partition row-index vectors), so we
// deliberately do NOT assert a sub-ms p99. The guard below is a generous "does
// not stall pathologically" bound the p99 clears with comfortable margin (the
// heaviest stage, `over`, sits near 17 ms p99, so 50 ms keeps the >=2x margin
// the org bar wants); the honest number to read is throughput, captured in
// perf/rust.json. Kept symmetric with the Java sibling, whose tail is GC-bound.
const GUARD_NS: u64 = 50_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        // 2k passes is plenty for a stable p99; `over` is heavy, so a larger
        // count just slows CI without tightening the tail.
        entries: 2_000,
        warmup: 500,
        seed: 7,
    };
    let h = run_bench(&WindowRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "lag",
                p99_ns_max: GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "cumsum",
                p99_ns_max: GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "over",
                p99_ns_max: GUARD_NS,
            },
        ],
    )
    .unwrap();
}
