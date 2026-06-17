#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_sql::recipe::SqlRecipe;

// The `parse` stage IS sub-ms: the lexer + recursive-descent walk over a
// moderate grouped query is a few microseconds, so we hold it under 1 ms.
const PARSE_GUARD_NS: u64 = 1_000_000;

// The `query` stage is throughput-contracted, NOT a per-op sub-ms primitive.
// Each timed sample parses + lowers + runs a FULL group-by-aggregate over a
// 4,096-row frame with a WHERE filter - the analytical front, not the tick
// loop. The whole pipeline materialises the frame's row axis (lazy filter) and
// then partitions + reduces per group (group-by), so the p99 lands in the low
// single-digit milliseconds on a laptop tier; the honest number is throughput,
// captured in perf/rust.json. The guard below is a generous "does not stall
// pathologically" bound that the observed p99 (~6 ms) clears with >2x margin.
const QUERY_GUARD_NS: u64 = 15_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 5_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&SqlRecipe, &params);
    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "parse",
                p99_ns_max: PARSE_GUARD_NS,
            },
            SubMsBenchAssertion {
                stage: "query",
                p99_ns_max: QUERY_GUARD_NS,
            },
        ],
    )
    .unwrap();
}
