package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import java.util.List;

/**
 * Asserts the recipe's throughput contract. Both stages are pure CPU work over
 * a 4,096-point series: {@code encode} builds the line-protocol batch; {@code
 * decode} tokenises an annotated CSV, parses RFC3339, and rebuilds a collection
 * (the heavier path - it allocates per row). Neither is a tick-loop per-op
 * primitive; the honest number is throughput, in perf/java.json. The guards are
 * generous no-pathological-stall bounds the observed p99 clears with margin.
 */
public final class SubmillisecondBench {
    private static final long ENCODE_GUARD_NS = 20_000_000L;
    private static final long DECODE_GUARD_NS = 60_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(1_000, 200, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new InfluxRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("encode", ENCODE_GUARD_NS),
                new SubMsBench.Assertion("decode", DECODE_GUARD_NS)));
        System.out.println("OK (encode + decode p99 under throughput guards)");
    }
}
