package com.submillisecond.recipes.arena;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.arena.features.GrowableArena;

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
        // GrowableArena so the operator doesn't have to pre-size for
        // every entries-count the bench is invoked with. After the
        // first round the buffer settles on a single sufficient chunk.
        GrowableArena arena = new GrowableArena(64 * 1024);
        for (int i = 0; i < warmup; i++) arena.allocate(8, 8);
        arena.reset();

        SubMsPerfHarness.Stage alloc = h.stage("allocate", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            arena.allocate(8, 8);
            alloc.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage reset = h.stage("reset", 1000).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < 1000; r++) {
            for (int i = 0; i < 100; i++) arena.allocate(8, 8);
            long t0 = SubMsTimer.nanosNow();
            arena.reset();
            reset.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("initial_capacity_bytes", "65536");
    }
}
