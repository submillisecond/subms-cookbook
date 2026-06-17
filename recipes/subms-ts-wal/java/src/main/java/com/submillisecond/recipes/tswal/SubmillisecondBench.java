package com.submillisecond.recipes.tswal;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(20_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new WalRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("append_buffered", ONE_MS_NS),
                new SubMsBench.Assertion("append_synced_n", ONE_MS_NS)));
        System.out.println("OK (append_buffered + append_synced_n p99 under 1 ms)");
    }
}
