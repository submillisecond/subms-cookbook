package com.submillisecond.recipes.tsarrow;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import java.io.IOException;

/**
 * stdin key=value params in, JSON on stdout. Run with
 * {@code --add-opens=java.base/java.nio=ALL-UNNAMED}.
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new ArrowRecipe(), params);
        h.meta("subms.recipe.slug", "subms-ts-adapter-arrow");
        h.meta("subms.recipe.category", "adapter");
        h.writeJson(System.out);
    }
}
