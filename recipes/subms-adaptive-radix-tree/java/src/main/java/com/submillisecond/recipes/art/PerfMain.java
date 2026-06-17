package com.submillisecond.recipes.art;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new ArtRecipe(), params);
        h.meta("subms.recipe.slug", "subms-adaptive-radix-tree");
        h.meta("subms.recipe.category", "ordered-index");
        h.writeJson(System.out);
    }
}
