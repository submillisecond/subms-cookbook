package com.submillisecond.recipes.timer;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;
    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(30_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new TimerWheelRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("schedule", ONE_MS_NS),
            new SubMsBench.Assertion("cancel",   ONE_MS_NS),
            new SubMsBench.Assertion("tick",     ONE_MS_NS)));
        System.out.println("OK (schedule + cancel + tick p99 under 1 ms)");
    }
}
