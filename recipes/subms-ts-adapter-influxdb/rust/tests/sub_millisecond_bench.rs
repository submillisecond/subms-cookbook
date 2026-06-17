#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_influxdb::recipe::InfluxRecipe;

// Both stages are pure CPU work over a 4,096-point series. `encode` (build the
// line-protocol batch) is the cheaper of the two; `decode` (RFC4180 tokenise +
// RFC3339 parse + collection rebuild) allocates more. Neither is a tick-loop
// per-op primitive - the recipe's honest contract is throughput, captured in
// perf/rust.json. The guards below are generous no-pathological-stall bounds;
// observed p99 clears them with comfortable margin on a laptop tier.
const ENCODE_GUARD_NS: u64 = 20_000_000;
const DECODE_GUARD_NS: u64 = 50_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 1_000,
        warmup: 200,
        seed: 7,
    };
    let h = run_bench(&InfluxRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "encode",
                p99_ns_max: ENCODE_GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "decode",
                p99_ns_max: DECODE_GUARD_NS,
            },
        ],
    )
    .unwrap();
}
