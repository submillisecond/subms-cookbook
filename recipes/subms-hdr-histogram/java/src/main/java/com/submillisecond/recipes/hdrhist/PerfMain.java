package com.submillisecond.recipes.hdrhist;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new HdrHistogramRecipe(), params);
        h.meta("subms.recipe.slug", "subms-hdr-histogram");
        h.meta("subms.recipe.category", "observability");
        h.writeJson(System.out);
    }
}
