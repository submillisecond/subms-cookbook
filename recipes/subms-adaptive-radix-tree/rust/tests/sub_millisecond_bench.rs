#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_adaptive_radix_tree::recipe::ArtRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 30_000,
        warmup: 1_000,
        seed: 0,
        ..Default::default()
    };
    let h = run_bench(&ArtRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "insert",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "lookup",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
