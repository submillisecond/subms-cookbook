#![cfg(feature = "harness")]

use subms::{assert_p99_under, run_bench, SubMsBenchAssertion, SubMsBenchParams};
use subms_block_cache::recipe::BlockCacheRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams { entries: 40_000, warmup: 1_000, seed: 0 };
    let h = run_bench(&BlockCacheRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion { stage: "get", p99_ns_max: ONE_MS_NS },
            SubMsBenchAssertion { stage: "put", p99_ns_max: ONE_MS_NS },
        ],
    ).unwrap();
}
