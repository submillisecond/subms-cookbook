package com.submillisecond.recipes.tsjoin;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Throughput-contracted recipe, NOT a per-op sub-ms primitive. Each timed
 * sample is a FULL join of two 4,096-row frames keyed on a STRING symbol - the
 * analytical front, not the tick loop. The tail is allocation / GC bound (the
 * hash index over key strings + the materialised output columns), so we
 * deliberately do NOT assert a sub-ms p99. The guard below is a generous "does
 * not stall pathologically" bound the p99 clears with margin; the honest number
 * to read is throughput, captured in perf/java.json. Kept symmetric with the
 * Rust sibling.
 */
public final class SubmillisecondBench {
    private static final long GUARD_NS = 40_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(5_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new JoinRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("hash_inner", GUARD_NS),
                new SubMsBench.Assertion("hash_outer", GUARD_NS)));
        System.out.println("OK (hash_inner + hash_outer p99 under the 40 ms throughput guard)");
    }
}
