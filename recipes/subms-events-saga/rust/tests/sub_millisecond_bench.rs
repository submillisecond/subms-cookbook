#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_events_saga::recipe::SagaRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 50_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&SagaRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "build",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "commit",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "compensate",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
