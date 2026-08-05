package com.submillisecond.recipes.art;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * The p99 gate CI actually runs. SubmillisecondBench is a main(), so surefire
 * never executes it and nothing stops a commit regressing the published number.
 * Params and assertions mirror that main exactly.
 */
final class SubmillisecondBenchTest {

    private static final long ONE_MS_NS = 1_000_000L;

    @Test
    void subMillisecondBench() {
        SubMsBenchParams params = new SubMsBenchParams(30_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new ArtRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("insert", ONE_MS_NS),
            new SubMsBench.Assertion("lookup", ONE_MS_NS)));
    }
}
