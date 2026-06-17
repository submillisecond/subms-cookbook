package com.submillisecond.recipes.tsjoin;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * stdin key=value params in, JSON on stdout.
 *
 * <pre>
 *   printf 'entries=20000\nwarmup=1000\nseed=7\n' | \
 *     java -cp target/classes:&lt;jars&gt; com.submillisecond.recipes.tsjoin.PerfMain
 * </pre>
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new JoinRecipe(), params);
        h.meta("subms.recipe.slug", "subms-ts-join");
        h.meta("subms.recipe.category", "timeseries");
        h.writeJson(System.out);
    }
}
