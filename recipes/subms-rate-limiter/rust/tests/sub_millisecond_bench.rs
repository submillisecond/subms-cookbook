#![cfg(feature = "harness")]

use subms::{assert_p99_under, run_bench, SubMsBenchAssertion, SubMsBenchParams};
use subms_rate_limiter::recipe::RateLimiterRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams { entries: 80_000, warmup: 1_000, seed: 0 };
    let h = run_bench(&RateLimiterRecipe, &params);
    assert_p99_under(
        &h,
        &[SubMsBenchAssertion { stage: "try_acquire", p99_ns_max: ONE_MS_NS }],
    ).unwrap();
}
