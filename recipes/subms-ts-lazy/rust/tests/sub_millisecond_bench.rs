#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_lazy::recipe::LazyRecipe;

// Two contracts of different kinds. `optimise_collect` is THROUGHPUT-contracted:
// each timed sample builds, optimises, and collects a whole 5-op pipeline (two
// filters, a derive, a sort, a project) over a 4,096-row frame - the analytical
// front, not the tick loop. Its p50 is multi-ms and its tail is alloc / GC bound
// (the aligned view materialises a boxed cell per row and the sort permutes
// every column), so we assert only a generous "does not stall pathologically"
// guard, NOT a tight p99; the honest number to read is throughput in
// perf/rust.json. `certify`, by contrast, is per-op work over the plan NODE LIST
// (independent of row count) and is genuinely sub-ms - so it gets a REAL sub-ms
// p99 assertion. That asymmetry is the recipe's whole thesis: you cannot promise
// a sub-ms collect, but you CAN emit a sub-ms-certified latency budget for it.
const COLLECT_GUARD_NS: u64 = 250_000_000;
const CERTIFY_P99_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 10_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&LazyRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "optimise_collect",
                p99_ns_max: COLLECT_GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "certify",
                p99_ns_max: CERTIFY_P99_NS,
            },
        ],
    )
    .unwrap();
}
