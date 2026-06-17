//! `SubMsRecipe` impl. Behind the `harness` feature (which implies `growable`).
//!
//! The bench drives the growable variant so the harness can run across a
//! wide entry range without the operator pre-sizing the buffer. After the
//! first round the arena converges on a single chunk that fits the steady
//! state.

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::GrowableBump;

/// Stages: `allocate`, `reset`. Allocates 8-byte values until full, then resets.
pub struct ArenaAllocatorRecipe;

impl SubMsRecipe for ArenaAllocatorRecipe {
    fn name(&self) -> &str {
        "arena-allocator"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let mut arena = GrowableBump::with_capacity(64 * 1024);
        for i in 0..warmup as u64 {
            let _ = arena.alloc_copy(i);
        }
        arena.reset();

        let s_alloc = h
            .stage("allocate", entries)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..entries as u64 {
            let t0 = SubMsTimer::tick();
            let _ = arena.alloc_copy(i);
            s_alloc.record(t0.elapsed_ns());
        }

        let s_reset = h.stage("reset", 1000).with_kind(SubMsStageKind::HotPath);
        for _ in 0..1000 {
            for i in 0..100u64 {
                let _ = arena.alloc_copy(i);
            }
            let t0 = SubMsTimer::tick();
            arena.reset();
            s_reset.record(t0.elapsed_ns());
        }

        h.add_meta("initial_capacity_bytes", "65536");
    }
}
