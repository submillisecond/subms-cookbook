#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_timer_wheel::recipe::TimerWheelRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 30_000,
        warmup: 1_000,
        seed: 0,
    };
    let h = run_bench(&TimerWheelRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "schedule",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "cancel",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "tick",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
