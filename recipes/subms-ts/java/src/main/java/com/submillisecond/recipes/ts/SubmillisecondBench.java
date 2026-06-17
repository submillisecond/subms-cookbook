package com.submillisecond.recipes.ts;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(50_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new TsRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("push", ONE_MS_NS),
                new SubMsBench.Assertion("nearest", ONE_MS_NS),
                new SubMsBench.Assertion("range_min", ONE_MS_NS),
                new SubMsBench.Assertion("range_sum", ONE_MS_NS)));
        System.out.println("OK (push + nearest + range_min + range_sum p99 under 1 ms)");
    }
}
