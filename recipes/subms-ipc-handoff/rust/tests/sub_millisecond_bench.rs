//! Sub-millisecond bench. Asserts p99 < 1 ms on the shared-memory stages.
//! The socket and SSE stages are measured, not budgeted: their cost is the
//! kernel's, and a loaded CI runner can push a loopback round trip past any
//! line drawn here.
#![cfg(feature = "harness")]

use subms::{SubMsBenchAssertion, SubMsBenchParams, assert_p99_under, run_bench};
use subms_ipc_handoff::recipe::IpcHandoffRecipe;

const ONE_MS_NS: u64 = 1_000_000;

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams {
        entries: 20_000,
        warmup: 1_000,
        seed: 0,
        ..Default::default()
    };
    let h = run_bench(&IpcHandoffRecipe, &params);

    let mut budget = vec![SubMsBenchAssertion {
        stage: "thread-ring-spin",
        p99_ns_max: ONE_MS_NS,
    }];
    if cfg!(target_os = "linux") {
        budget.push(SubMsBenchAssertion {
            stage: "process-shm-spin",
            p99_ns_max: ONE_MS_NS,
        });
    }
    assert_p99_under(&h, &budget).unwrap();
}
