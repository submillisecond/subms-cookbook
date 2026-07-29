#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_count_min_sketch::recipe::CountMinSketchRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 50_000,
        warmup: 1_000,
        seed: 0,
        ..Default::default()
    };
    let h = run_bench(&CountMinSketchRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "add",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "estimate",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
