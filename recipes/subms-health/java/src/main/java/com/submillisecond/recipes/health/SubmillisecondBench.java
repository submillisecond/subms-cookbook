package com.submillisecond.recipes.health;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(50_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new HealthRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("register", ONE_MS_NS),
                new SubMsBench.Assertion("render_json", ONE_MS_NS)));
        System.out.println("OK (register + render_json p99 under 1 ms)");
    }
}
