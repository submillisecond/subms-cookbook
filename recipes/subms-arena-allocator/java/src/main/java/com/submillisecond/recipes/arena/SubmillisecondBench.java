package com.submillisecond.recipes.arena;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;
    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(100_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new ArenaAllocatorRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("allocate", ONE_MS_NS),
            new SubMsBench.Assertion("reset",    ONE_MS_NS)));
        System.out.println("OK (allocate + reset p99 under 1 ms)");
    }
}
