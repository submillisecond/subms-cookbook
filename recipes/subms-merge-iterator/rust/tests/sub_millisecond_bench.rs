#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_merge_iterator::recipe::MergeIteratorRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 160_000,
        warmup: 0,
        seed: 0,
        ..Default::default()
    };
    let h = run_bench(&MergeIteratorRecipe, &params);
    assert_p99_under(
        &h,
        &[SubMsBenchAssertion {
            stage: "next",
            p99_ns_max: ONE_MS_NS,
        }],
    )
    .unwrap();
}
