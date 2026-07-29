#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_events_store::recipe::EventStoreRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 50_000,
        warmup: 1_000,
        seed: 7,
        ..Default::default()
    };
    let h = run_bench(&EventStoreRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "append",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "replay",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "catch_up",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
