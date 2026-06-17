#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_cbor::recipe::CborRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 20_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&CborRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "encode",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "decode",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
