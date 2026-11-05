package com.submillisecond.recipes.fixedpoint;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/** Asserts p99 under 1 ms on add, mul, div and round_tick. */
public final class SubmillisecondBench {

    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(100_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new FixedPointRecipe(), params);

        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("add", ONE_MS_NS),
            new SubMsBench.Assertion("mul", ONE_MS_NS),
            new SubMsBench.Assertion("div", ONE_MS_NS),
            new SubMsBench.Assertion("round_tick", ONE_MS_NS)));

        System.out.println("OK (add + mul + div + round_tick p99 under 1 ms)");
    }
}
