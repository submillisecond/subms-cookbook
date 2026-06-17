#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_asof_join::recipe::AsofJoinRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 20_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&AsofJoinRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "join_backward",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "join_nearest",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
