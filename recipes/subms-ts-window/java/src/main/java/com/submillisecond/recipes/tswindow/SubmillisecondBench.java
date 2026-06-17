package com.submillisecond.recipes.tswindow;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed sample
 * is a FULL window pass over a partitioned 4,096-row frame: a partition grouping,
 * a per-partition sort/scan, and (for {@code over}) a per-partition aggregate.
 * This is the analytical front, not the tick loop. The typical (p50) whole-frame
 * pass is sub-ms, but the tail is allocation / GC bound (each pass materialises
 * columns + per-partition row-index arrays), so we deliberately do NOT assert a
 * sub-ms p99. The guard below is a generous "does not stall pathologically" bound
 * the p99 clears with comfortable margin (the heaviest stage, {@code over}, sits
 * near 17 ms p99, so 50 ms keeps the >=2x margin the org bar wants); the honest
 * number to read is throughput, captured in perf/java.json. Kept symmetric with
 * the Rust sibling.
 */
public final class SubmillisecondBench {
    private static final long GUARD_NS = 50_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(2_000, 500, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new WindowRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("lag", GUARD_NS),
                new SubMsBench.Assertion("cumsum", GUARD_NS),
                new SubMsBench.Assertion("over", GUARD_NS)));
        System.out.println("OK (lag + cumsum + over p99 under the 50 ms throughput guard)");
    }
}
