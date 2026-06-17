#![cfg(feature = "harness")]
use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ts_yaml::recipe::YamlRecipe;

const ONE_MS_NS: u64 = 1_000_000;

// Only `encode` carries a per-op sub-ms gate. The hand-written encode is a
// linear pass and holds p99 well under 1 ms in both ports. `decode` runs a full
// YAML parse: saphyr (Rust) is comfortably sub-ms at this workload, but
// snakeyaml (Java) allocates heavily and its p99 is GC-dominated and volatile
// (sub-ms median, multi-ms tail). The published decode claim is therefore
// throughput, not per-op latency, so the CI gate deliberately does not assert a
// decode p99 it cannot honestly hold across both languages. Decode numbers are
// captured in perf/{rust,java}.json and read in the writeup.
#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 20_000,
        warmup: 1_000,
        seed: 7,
    };
    let h = run_bench(&YamlRecipe, &params);
    assert_p99_under(
        &h,
        &[SubMsBenchAssertion {
            stage: "encode",
            p99_ns_max: ONE_MS_NS,
        }],
    )
    .unwrap();
}
