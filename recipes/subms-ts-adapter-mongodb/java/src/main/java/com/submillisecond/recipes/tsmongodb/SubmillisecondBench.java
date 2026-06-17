package com.submillisecond.recipes.tsmongodb;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import java.util.List;

/**
 * Asserts the recipe's per-op sub-ms claim. Each stage is the cost of mapping
 * ONE point document - {@code encode} to BSON, {@code decode} back - the
 * primitive a tick loop pays per observation. Both assert p99 &lt; 1 ms; the
 * observed p99 clears it by orders of magnitude. Whole-batch bulk throughput is
 * a separate, reported number in perf/java.json + the writeup.
 */
public final class SubmillisecondBench {
    private static final long ENCODE_GUARD_NS = 1_000_000L;
    private static final long DECODE_GUARD_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(1_000, 200, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new MongoRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("encode", ENCODE_GUARD_NS),
                new SubMsBench.Assertion("decode", DECODE_GUARD_NS)));
        System.out.println("OK (encode + decode p99 under throughput guards)");
    }
}
