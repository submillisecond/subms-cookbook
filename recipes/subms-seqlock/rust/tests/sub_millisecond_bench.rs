//! Sub-millisecond bench.
#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_seqlock::recipe::SeqLockRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 100_000,
        warmup: 1_000,
        seed: 0,
        ..Default::default()
    };
    let h = run_bench(&SeqLockRecipe, &params);

    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "write",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "read",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
