package com.submillisecond.recipes.tsasofjoin;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(20_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new AsofJoinRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("join_backward", ONE_MS_NS),
                new SubMsBench.Assertion("join_nearest", ONE_MS_NS)));
        System.out.println("OK (join_backward + join_nearest p99 under 1 ms)");
    }
}
