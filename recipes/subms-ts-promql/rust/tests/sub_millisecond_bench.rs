#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_promql::recipe::PromQlRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 5_000,
        warmup: 500,
        seed: 7,
    };
    let h = run_bench(&PromQlRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "parse",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "eval",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
