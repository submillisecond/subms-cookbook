package com.submillisecond.recipes.bloom;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;
    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(100_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new BloomFilterRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("add",                ONE_MS_NS),
            new SubMsBench.Assertion("might_contain_hit",  ONE_MS_NS),
            new SubMsBench.Assertion("might_contain_miss", ONE_MS_NS)));
        System.out.println("OK (add + might_contain_hit + might_contain_miss p99 under 1 ms)");
    }
}
