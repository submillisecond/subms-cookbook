#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_join::recipe::JoinRecipe;

// Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
// sample is a FULL join of two 4,096-row frames keyed on a STRING symbol - the
// analytical front, not the tick loop. The tail is allocation-bound (the hash
// index over owned key strings + the materialised output columns), so we
// deliberately do NOT assert a sub-ms p99. The guard below is a generous "does
// not stall pathologically" bound the p99 clears with margin; the honest number
// to read is throughput, captured in perf/rust.json. Kept symmetric with the
// Java sibling, whose tail is GC-bound.
const GUARD_NS: u64 = 40_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 5_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&JoinRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "hash_inner",
                p99_ns_max: GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "hash_outer",
                p99_ns_max: GUARD_NS,
            },
        ],
    )
    .unwrap();
}
