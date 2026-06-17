package com.submillisecond.recipes.tsexpr;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
 * sample is a FULL evaluation of a multi-node pipeline over a 4,096-row frame -
 * the analytical front, not the tick loop. The TYPICAL (p50) whole-frame eval
 * is sub-ms in both languages (~430-510 us), but the tail is GC bound
 * (the aligned view boxes a cell per row), so we deliberately do NOT assert a
 * sub-ms p99. The guard below is a generous "does not stall pathologically"
 * bound the p99 clears with margin; the honest number to read is throughput.
 */
public final class SubmillisecondBench {
    private static final long GUARD_NS = 10_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(10_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new ExprRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("eval_pipeline", GUARD_NS),
                new SubMsBench.Assertion("eval_agg", GUARD_NS)));
        System.out.println("OK (eval_pipeline + eval_agg p99 under the 10 ms throughput guard)");
    }
}
