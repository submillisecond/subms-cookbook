#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_arena_allocator::recipe::ArenaAllocatorRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 100_000,
        warmup: 1_000,
        seed: 0,
    };
    let h = run_bench(&ArenaAllocatorRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "allocate",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "reset",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
