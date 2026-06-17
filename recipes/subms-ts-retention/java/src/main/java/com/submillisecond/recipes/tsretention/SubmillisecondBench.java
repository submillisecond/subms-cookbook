package com.submillisecond.recipes.tsretention;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(10_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new RetentionRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("apply_age", ONE_MS_NS),
                new SubMsBench.Assertion("apply_count", ONE_MS_NS)));
        System.out.println("OK (apply_age + apply_count p99 under 1 ms)");
    }
}
