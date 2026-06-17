#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_parquet::recipe::ParquetRecipe;

// Per-op: encode / decode a 256-point series to / from a Parquet file. Parquet
// does real work (row groups, column chunks, page headers, a thrift footer), so
// the claim is scoped to a modest series where the whole round trip still clears
// p99 < 1 ms. Larger files are reported throughput in perf, not asserted.
const ENCODE_GUARD_NS: u64 = 1_000_000;
const DECODE_GUARD_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 1_000,
        warmup: 200,
        seed: 7,
    };
    let h = run_bench(&ParquetRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "encode",
                p99_ns_max: ENCODE_GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "decode",
                p99_ns_max: DECODE_GUARD_NS,
            },
        ],
    )
    .unwrap();
}
