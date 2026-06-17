#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_mongodb::recipe::MongoRecipe;

// Per-op primitive: encode / decode ONE point document - the cost a tick loop
// pays per observation. This IS a sub-ms claim: each stage asserts p99 < 1 ms,
// and the observed p99 clears it by orders of magnitude (sub-microsecond on a
// laptop tier). Whole-batch bulk throughput is a separate, reported number in
// perf/rust.json + the writeup, not asserted here.
const ENCODE_GUARD_NS: u64 = 1_000_000;
const DECODE_GUARD_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 1_000,
        warmup: 200,
        seed: 7,
    };
    let h = run_bench(&MongoRecipe, &params);
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
