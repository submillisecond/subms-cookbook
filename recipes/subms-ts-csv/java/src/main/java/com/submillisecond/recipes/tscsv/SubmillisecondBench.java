package com.submillisecond.recipes.tscsv;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Throughput-contracted gate for the CSV read / write block. A 4,096-row,
 * 5-column whole-frame parse is O(rows) and runs in milliseconds, not
 * microseconds, so these are generous block-latency bounds with headroom over
 * the measured p99 on the laptop tier - the gate catches an order-of-magnitude
 * regression without pretending the parse is sub-millisecond.
 */
public final class SubmillisecondBench {
    private static final long READ_NS_MAX = 50_000_000L;
    private static final long WRITE_NS_MAX = 30_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(10_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new CsvRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("read", READ_NS_MAX),
                new SubMsBench.Assertion("write", WRITE_NS_MAX)));
        System.out.println("OK (read + write p99 under the throughput bound)");
    }
}
