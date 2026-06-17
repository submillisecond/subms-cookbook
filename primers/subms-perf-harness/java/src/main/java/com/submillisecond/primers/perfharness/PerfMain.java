package com.submillisecond.primers.perfharness;

import java.io.IOException;
import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * stdin key=value params in, JSON on stdout. Drives {@link HarnessRecipe}
 * through the cookbook harness end-to-end:
 *
 * <ol>
 *   <li>parse params from stdin (or fall back to defaults)</li>
 *   <li>{@link SubMsBench#runBench} returns a populated harness</li>
 *   <li>{@link SubMsBench#summarize} sorts samples and computes percentiles</li>
 *   <li>{@link SubMsBench#printSummary} renders a fixed-width table to stderr</li>
 *   <li>{@link SubMsBench#assertP99Under} gates p99 &lt; 1 ms per stage</li>
 *   <li>{@link SubMsBench#summaryToJson} emits the canonical JSON to stdout</li>
 * </ol>
 *
 * The split between stderr (human-readable table) and stdout (JSON) is
 * deliberate: callers can pipe stdout to {@code perf/java.json} and still
 * see the run on the console.
 *
 * <pre>
 *   echo "entries=20000" | java -cp target/classes \
 *       com.submillisecond.primers.perfharness.PerfMain &gt; perf/java.json
 * </pre>
 */
public final class PerfMain {

    private static final long ONE_MS_NS = 1_000_000L;

    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();

        SubMsPerfHarness h = SubMsBench.runBench(new HarnessRecipe(), params);
        SubMsBenchSummary summary = SubMsBench.summarize(h);

        SubMsBench.printSummary(summary, System.err);

        SubMsBench.assertP99Under(summary, List.of(
                new SubMsBench.Assertion("put",      ONE_MS_NS),
                new SubMsBench.Assertion("get_hit",  ONE_MS_NS),
                new SubMsBench.Assertion("get_miss", ONE_MS_NS)));

        SubMsBench.summaryToJson(summary, System.out);
    }

    private PerfMain() {}
}
