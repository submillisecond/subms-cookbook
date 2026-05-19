//! `SubMsRecipe` impl. Behind the `harness` feature.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe};

use crate::Bump;

/// Stages: `allocate`, `reset`. Allocates 8-byte values until full, then resets.
pub struct ArenaAllocatorRecipe;

impl SubMsRecipe for ArenaAllocatorRecipe {
    fn name(&self) -> &str {
        "arena-allocator"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let mut arena = Bump::with_capacity(64 * 1024);
        for i in 0..warmup as u64 {
            let _ = arena.alloc_copy(i);
        }
        arena.reset();

        let s_alloc = h.stage("allocate", entries);
        for i in 0..entries as u64 {
            let t0 = Instant::now();
            let _ = arena.alloc_copy(i);
            s_alloc.record(t0.elapsed().as_nanos() as u64);
        }

        let s_reset = h.stage("reset", 1000);
        for _ in 0..1000 {
            for i in 0..100u64 { let _ = arena.alloc_copy(i); }
            let t0 = Instant::now();
            arena.reset();
            s_reset.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("initial_capacity_bytes", "65536");
    }
}
