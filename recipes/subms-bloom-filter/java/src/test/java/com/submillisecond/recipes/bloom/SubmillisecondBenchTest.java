package com.submillisecond.recipes.bloom;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * The p99 gate CI actually runs. SubmillisecondBench is a main(), so surefire
 * never executes it and nothing stops a commit regressing the published number.
 */
final class SubmillisecondBenchTest {

    private static final long ONE_MS_NS = 1_000_000L;

    @Test
    void subMillisecondBench() {
        SubMsBenchParams params = new SubMsBenchParams(100_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new BloomFilterRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("add", ONE_MS_NS),
            new SubMsBench.Assertion("might_contain_hit", ONE_MS_NS),
            new SubMsBench.Assertion("might_contain_miss", ONE_MS_NS)));
    }
}
