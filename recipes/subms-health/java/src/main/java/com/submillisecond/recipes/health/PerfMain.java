package com.submillisecond.recipes.health;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new HealthRecipe(), params);
        h.meta("subms.recipe.slug", "subms-health");
        h.meta("subms.recipe.category", "observability");
        h.writeJson(System.out);
    }
}
