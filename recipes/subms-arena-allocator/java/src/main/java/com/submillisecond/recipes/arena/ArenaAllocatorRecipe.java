package com.submillisecond.recipes.arena;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/** Stages: {@code allocate}, {@code reset}. */
public final class ArenaAllocatorRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "arena-allocator";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        BumpArena arena = new BumpArena(64 * 1024);
        for (int i = 0; i < warmup; i++) arena.allocate(8, 8);
        arena.reset();

        SubMsPerfHarness.Stage alloc = h.stage("allocate", entries);
        for (int i = 0; i < entries; i++) {
            long t0 = System.nanoTime();
            arena.allocate(8, 8);
            alloc.record(System.nanoTime() - t0);
        }

        SubMsPerfHarness.Stage reset = h.stage("reset", 1000);
        for (int r = 0; r < 1000; r++) {
            for (int i = 0; i < 100; i++) arena.allocate(8, 8);
            long t0 = System.nanoTime();
            arena.reset();
            reset.record(System.nanoTime() - t0);
        }

        h.meta("initial_capacity_bytes", "65536");
    }
}
