#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_segment_reader::recipe::SegmentReaderRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 100_000,
        warmup: 0,
        seed: 0,
        ..Default::default()
    };
    let h = run_bench(&SegmentReaderRecipe, &params);
    assert_p99_under(
        &h,
        &[SubMsBenchAssertion {
            stage: "next_record",
            p99_ns_max: ONE_MS_NS,
        }],
    )
    .unwrap();
}
