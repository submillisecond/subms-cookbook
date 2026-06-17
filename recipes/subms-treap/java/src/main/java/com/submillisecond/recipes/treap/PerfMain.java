package com.submillisecond.recipes.treap;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new TreapRecipe(), params);
        h.meta("subms.recipe.slug", "subms-treap");
        h.meta("subms.recipe.category", "ordered-index");
        h.writeJson(System.out);
    }
}
