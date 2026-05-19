//! Sub-millisecond bench. Asserts p99 < 1 ms on enqueue and dequeue.
#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_spsc_ring_buffer::recipe::SpscRingBufferRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 100_000,
        warmup: 1_000,
        seed: 0,
    };
    let h = run_bench(&SpscRingBufferRecipe, &params);

    assert_p99_under(
        &h,
        &[
            SubMsBenchAssertion {
                stage: "enqueue",
                p99_ns_max: ONE_MS_NS,
            },
            SubMsBenchAssertion {
                stage: "dequeue",
                p99_ns_max: ONE_MS_NS,
            },
        ],
    )
    .unwrap();
}
