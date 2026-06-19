#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_events::recipe::EventsRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 50_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&EventsRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "build",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "emit_sync",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "emit_async",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
