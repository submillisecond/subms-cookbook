package com.submillisecond.recipes.tsreshape;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed sample
 * is a FULL reshape - a long-to-wide pivot of a 4,096-row frame into a 256x16
 * grid keyed on a STRING category, or a wide-to-long melt of a 4,096-row frame
 * into ROWS*4 long rows with a Str variable column. This is the analytical
 * front, not the tick loop. The typical (p50) whole-frame reshape is sub-ms
 * here, but the tail is allocation / GC bound (the bucket map + the materialised
 * output columns), so we deliberately do NOT assert a sub-ms p99. The guard
 * below is a generous "does not stall pathologically" bound the p99 clears with
 * margin; the honest number to read is throughput, captured in perf/java.json.
 * Kept symmetric with the Rust sibling.
 */
public final class SubmillisecondBench {
    private static final long GUARD_NS = 40_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(5_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new ReshapeRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("pivot", GUARD_NS),
                new SubMsBench.Assertion("melt", GUARD_NS)));
        System.out.println("OK (pivot + melt p99 under the 40 ms throughput guard)");
    }
}
