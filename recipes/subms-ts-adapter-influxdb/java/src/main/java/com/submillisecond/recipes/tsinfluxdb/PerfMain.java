package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import java.io.IOException;

/**
 * stdin key=value params in, JSON on stdout.
 *
 * <pre>
 *   printf 'entries=2000\nwarmup=500\nseed=7\n' | \
 *     java -cp target/classes:&lt;jars&gt; com.submillisecond.recipes.tsinfluxdb.PerfMain
 * </pre>
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new InfluxRecipe(), params);
        h.meta("subms.recipe.slug", "subms-ts-adapter-influxdb");
        h.meta("subms.recipe.category", "timeseries");
        h.writeJson(System.out);
    }
}
