package com.submillisecond.recipes.tsgroupby;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
 * sample is a FULL group-by-aggregate over a 4,096-row frame keyed by a
 * low-cardinality column - the analytical front, not the tick loop. The typical
 * (p50) whole-frame group-by is sub-ms, but the tail is allocation / GC bound
 * (a sub-frame is materialised per group, then the expr evaluator walks each),
 * so we deliberately do NOT assert a sub-ms p99. The guard below is a generous
 * "does not stall pathologically" bound the p99 clears with margin; the honest
 * number to read is throughput, captured in perf/java.json. Kept symmetric with
 * the Rust sibling.
 */
public final class SubmillisecondBench {
    private static final long GUARD_NS = 50_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(5_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new GroupByRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("group_agg", GUARD_NS),
                new SubMsBench.Assertion("value_counts", GUARD_NS)));
        System.out.println("OK (group_agg + value_counts p99 under the 50 ms throughput guard)");
    }
}
