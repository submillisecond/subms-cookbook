package com.submillisecond.recipes.lsm;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsBenchSummary;

/**
 * Runs the LSM recipe under both bloom modes back-to-back, prints the
 * formatted percentile table from the shared harness, and asserts
 * p99 under 1 ms for the {@link BloomMode#ON} pass.
 *
 * <pre>
 *   java -cp ... com.submillisecond.recipes.lsm.SubmillisecondBench
 * </pre>
 *
 * <p>The same code in Rust lives at
 * {@code recipes/subms-lsm-tree/rust/tests/sub_millisecond_bench.rs}; the
 * output is byte-equivalent modulo per-run jitter.
 */
public final class SubmillisecondBench {

    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(50_000, 5_000, 0L);

        SubMsBenchSummary on  = SubMsBench.summarizeLean(
                SubMsBench.runBench(new LsmTreeRecipe(16_000, BloomMode.ON),  params));
        SubMsBenchSummary off = SubMsBench.summarizeLean(
                SubMsBench.runBench(new LsmTreeRecipe(16_000, BloomMode.OFF), params));

        System.out.printf("entries=%d  flush_threshold_bytes=%d  warmup=%d%n%n",
                params.entries(), 16_000, params.warmup());
        System.out.println("bloom = on");
        SubMsBench.printSummary(on, System.out);
        System.out.println();
        System.out.println("bloom = off");
        SubMsBench.printSummary(off, System.out);

        SubMsBench.assertP99Under(on, List.of(
                new SubMsBench.Assertion("put",      ONE_MS_NS),
                new SubMsBench.Assertion("get_hit",  ONE_MS_NS),
                new SubMsBench.Assertion("get_miss", ONE_MS_NS)));

        System.out.println();
        System.out.println("OK (BloomMode.ON - all p99 < 1ms)");
    }
}
