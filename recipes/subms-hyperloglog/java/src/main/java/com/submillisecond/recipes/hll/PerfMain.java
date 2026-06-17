package com.submillisecond.recipes.hll;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new HyperLogLogRecipe(), params);
        h.meta("subms.recipe.slug", "subms-hyperloglog");
        h.meta("subms.recipe.category", "probabilistic");
        h.writeJson(System.out);
    }
}
