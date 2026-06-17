#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_csv::recipe::CsvRecipe;

// read + write of a 4096-row, 5-column block. This is a THROUGHPUT-contracted
// recipe, not a per-call sub-ms one: a whole-frame parse of thousands of rows
// is inherently O(rows) and runs in milliseconds, not microseconds. The honest
// per-row figure is roughly a microsecond; the block latency is a few
// milliseconds. These guards are generous bounds with headroom over the
// measured p99 on the laptop tier, so the gate catches an order-of-magnitude
// regression without pretending a 4096-row parse is sub-millisecond.
const READ_NS_MAX: u64 = 50_000_000;
const WRITE_NS_MAX: u64 = 30_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 10_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&CsvRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "read",
                p99_ns_max: READ_NS_MAX,
            },
            SubMsBenchAssertion {
                stage: "write",
                p99_ns_max: WRITE_NS_MAX,
            },
        ],
    )
    .unwrap();
}
