package com.submillisecond.recipes.tsplan;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(20_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new PlanRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("certify", ONE_MS_NS),
                new SubMsBench.Assertion("verify", ONE_MS_NS)));
        System.out.println("OK (certify + verify p99 under 1 ms)");
    }
}
