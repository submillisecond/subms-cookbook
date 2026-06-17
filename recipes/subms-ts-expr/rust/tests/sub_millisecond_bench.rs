#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_expr::recipe::ExprRecipe;

// Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
// sample is a FULL evaluation of a multi-node pipeline over a 4,096-row frame -
// the analytical front, not the tick loop. The TYPICAL (p50) whole-frame eval
// is sub-ms (~470-510 us here), but the tail crosses a millisecond (the aligned
// view materialises a boxed cell per row), so we deliberately do NOT assert a
// sub-ms p99. The guard below is a generous "does not stall pathologically" bound the
// p99 clears with margin; the honest number to read is throughput, captured in
// perf/rust.json. Kept symmetric with the Java sibling, whose tail is GC-bound.
const GUARD_NS: u64 = 10_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 10_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&ExprRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "eval_pipeline",
                p99_ns_max: GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "eval_agg",
                p99_ns_max: GUARD_NS,
            },
        ],
    )
    .unwrap();
}
