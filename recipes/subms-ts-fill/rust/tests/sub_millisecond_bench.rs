#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_fill::recipe::FillRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 20_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&FillRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "fill_linear",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "fill_locf",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
