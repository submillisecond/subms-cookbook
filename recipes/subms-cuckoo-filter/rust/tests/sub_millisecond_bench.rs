#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_cuckoo_filter::recipe::CuckooFilterRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 30_000,
        warmup: 1_000,
        seed: 0,
    };
    let h = run_bench(&CuckooFilterRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "insert",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "contains",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "delete",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
