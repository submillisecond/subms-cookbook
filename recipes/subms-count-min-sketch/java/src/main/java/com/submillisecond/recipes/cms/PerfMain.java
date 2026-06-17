package com.submillisecond.recipes.cms;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new CountMinSketchRecipe(), params);
        h.meta("subms.recipe.slug", "subms-count-min-sketch");
        h.meta("subms.recipe.category", "probabilistic");
        h.writeJson(System.out);
    }
}
