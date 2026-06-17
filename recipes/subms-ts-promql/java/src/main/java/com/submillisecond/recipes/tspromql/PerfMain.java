package com.submillisecond.recipes.tspromql;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * stdin key=value params in, JSON on stdout.
 *
 * <pre>
 *   printf 'entries=10000\nwarmup=1000\nseed=7\n' | \
 *     java -cp target/classes:&lt;jars&gt; com.submillisecond.recipes.tspromql.PerfMain
 * </pre>
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new PromQlRecipe(), params);
        h.meta("subms.recipe.slug", "subms-ts-promql");
        h.meta("subms.recipe.category", "timeseries");
        h.writeJson(System.out);
    }
}
