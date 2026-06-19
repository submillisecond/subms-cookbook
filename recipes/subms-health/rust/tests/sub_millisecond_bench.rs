#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_health::recipe::HealthRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 50_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&HealthRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "register",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "report",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "render_json",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
