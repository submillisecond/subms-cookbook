#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_wal::recipe::WalRecipe;
const ONE_MS_NS: u64 = 1_000_000;
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 20_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&WalRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "append_buffered",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "append_synced_n",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
