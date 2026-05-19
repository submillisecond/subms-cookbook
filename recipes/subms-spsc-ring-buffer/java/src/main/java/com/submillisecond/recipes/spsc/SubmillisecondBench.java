package com.submillisecond.recipes.spsc;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/** Asserts p99 under 1 ms on enqueue and dequeue. */
public final class SubmillisecondBench {

    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(100_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new SpscRingBufferRecipe(), params);

        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("enqueue", ONE_MS_NS),
            new SubMsBench.Assertion("dequeue", ONE_MS_NS)));

        System.out.println("OK (enqueue + dequeue p99 under 1 ms)");
    }
}
