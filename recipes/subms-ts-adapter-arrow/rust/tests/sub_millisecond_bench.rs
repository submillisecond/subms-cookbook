#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_arrow::recipe::ArrowRecipe;

// Per-op primitive: convert a whole 4,096-point series to / from an Arrow
// RecordBatch. The columnar build is two bulk buffer fills, not per-element
// allocation, so this clears p99 < 1 ms with margin. IPC framing is reported in
// perf, not asserted here.
const TO_GUARD_NS: u64 = 1_000_000;
const FROM_GUARD_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 1_000,
        warmup: 200,
        seed: 7,
    };
    let h = run_bench(&ArrowRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "to_batch",
                p99_ns_max: TO_GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "from_batch",
                p99_ns_max: FROM_GUARD_NS,
            },
        ],
    )
    .unwrap();
}
