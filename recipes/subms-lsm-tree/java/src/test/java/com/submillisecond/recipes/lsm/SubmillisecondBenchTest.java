package com.submillisecond.recipes.lsm;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsBenchParams;

/**
 * The p99 gate CI actually runs.
 *
 * <p>{@link SubmillisecondBench} is a {@code main()}, so surefire never executes
 * it - the file looked like a gate while nothing stopped a commit regressing the
 * published Java number. Same params and the same assertions as the main so the
 * two cannot drift.
 *
 * <p>Asserts the {@link BloomMode#ON} pass only, matching the main: with the
 * filter off, a miss walks every SSTable and is not the configuration the claim
 * is made under.
 */
final class SubmillisecondBenchTest {

    private static final long ONE_MS_NS = 1_000_000L;

    @Test
    void subMillisecondBench() {
        SubMsBenchParams params = new SubMsBenchParams(50_000, 5_000, 0L);
        SubMsBenchSummary on = SubMsBench.summarizeLean(
                SubMsBench.runBench(new LsmTreeRecipe(16_000, BloomMode.ON), params));
        SubMsBench.assertP99Under(on, List.of(
                new SubMsBench.Assertion("put",      ONE_MS_NS),
                new SubMsBench.Assertion("get_hit",  ONE_MS_NS),
                new SubMsBench.Assertion("get_miss", ONE_MS_NS)));
    }
}
