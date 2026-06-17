package com.submillisecond.recipes.tslazy;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Two contracts of different kinds. {@code optimise_collect} is
 * THROUGHPUT-contracted: each timed sample builds, optimises, and collects a
 * whole 5-op pipeline (two filters, a derive, a sort, a project) over a 4,096-row
 * frame - the analytical front, not the tick loop. Its tail is alloc / GC bound
 * (the aligned view boxes a cell per row and the sort permutes every column), so
 * it gets only a generous "does not stall pathologically" guard, NOT a tight
 * p99; the honest number to read is throughput. {@code certify}, by contrast, is
 * per-op work over the plan NODE LIST (independent of row count) and is genuinely
 * sub-ms, so it gets a REAL sub-ms p99 assertion. That asymmetry is the recipe's
 * thesis: you cannot promise a sub-ms collect, but you CAN emit a
 * sub-ms-certified latency budget for it.
 */
public final class SubmillisecondBench {
    private static final long COLLECT_GUARD_NS = 250_000_000L;
    private static final long CERTIFY_P99_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(10_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new LazyRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("optimise_collect", COLLECT_GUARD_NS),
                new SubMsBench.Assertion("certify", CERTIFY_P99_NS)));
        System.out.println("OK (optimise_collect under guard; certify sub-ms)");
    }
}
