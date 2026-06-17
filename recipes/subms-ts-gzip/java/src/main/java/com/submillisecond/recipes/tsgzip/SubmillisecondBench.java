package com.submillisecond.recipes.tsgzip;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(20_000, 5_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new GzipRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("encode", ONE_MS_NS),
                new SubMsBench.Assertion("decode", ONE_MS_NS)));
        System.out.println("OK (encode + decode p99 under 1 ms)");
    }
}
