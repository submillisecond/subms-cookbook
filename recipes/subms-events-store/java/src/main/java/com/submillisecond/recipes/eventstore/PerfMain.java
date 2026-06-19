package com.submillisecond.recipes.eventstore;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new EventStoreRecipe(), params);
        h.meta("subms.recipe.slug", "subms-events-store");
        h.meta("subms.recipe.category", "storage");
        h.writeJson(System.out);
    }
}
