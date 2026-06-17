package com.submillisecond.recipes.bloom;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * stdin key=value params in, JSON on stdout.
 *
 * <pre>
 *   cat &lt;&lt;EOF | java -cp ... com.submillisecond.recipes.bloom.PerfMain
 *   entries=50000
 *   warmup=5000
 *   seed=0
 *   EOF
 * </pre>
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new BloomFilterRecipe(), params);
        h.meta("subms.recipe.slug", "subms-bloom-filter");
        h.meta("subms.recipe.category", "probabilistic");
        h.writeJson(System.out);
    }
}
