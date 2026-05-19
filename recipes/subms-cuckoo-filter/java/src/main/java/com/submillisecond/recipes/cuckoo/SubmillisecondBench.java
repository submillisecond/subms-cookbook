package com.submillisecond.recipes.cuckoo;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;
    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(30_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new CuckooFilterRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("insert",   ONE_MS_NS),
            new SubMsBench.Assertion("contains", ONE_MS_NS),
            new SubMsBench.Assertion("delete",   ONE_MS_NS)));
        System.out.println("OK (insert + contains + delete p99 under 1 ms)");
    }
}
