package com.submillisecond.recipes.tsparquet;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import java.io.IOException;

/** stdin key=value params in, JSON on stdout. */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new ParquetRecipe(), params);
        h.meta("subms.recipe.slug", "subms-ts-adapter-parquet");
        h.meta("subms.recipe.category", "adapter");
        h.writeJson(System.out);
    }
}
