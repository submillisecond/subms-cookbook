package com.submillisecond.recipes.eventstore;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(50_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new EventStoreRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("append", ONE_MS_NS),
                new SubMsBench.Assertion("replay", ONE_MS_NS),
                new SubMsBench.Assertion("catch_up", ONE_MS_NS)));
        System.out.println("OK (append + replay + catch_up p99 under 1 ms)");
    }
}
