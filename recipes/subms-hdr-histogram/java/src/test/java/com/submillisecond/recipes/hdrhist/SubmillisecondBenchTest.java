package com.submillisecond.recipes.hdrhist;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * The p99 gate CI actually runs.
 *
 * <p>{@link SubmillisecondBench} is a {@code main()}, so surefire never executes
 * it - the file looked like a gate while nothing stopped a commit regressing the
 * published Java number. Same params as the main so the two cannot drift.
 */
final class SubmillisecondBenchTest {

    private static final long ONE_MS_NS = 1_000_000L;

    @Test
    void subMillisecondBench() {
        SubMsBenchParams params = new SubMsBenchParams(100_000, 1_000, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new HdrHistogramRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
            new SubMsBench.Assertion("record",     ONE_MS_NS),
            new SubMsBench.Assertion("percentile", ONE_MS_NS)));
    }
}
