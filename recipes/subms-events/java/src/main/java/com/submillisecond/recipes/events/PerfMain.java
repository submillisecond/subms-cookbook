package com.submillisecond.recipes.events;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new EventsRecipe(), params);
        h.meta("subms.recipe.slug", "subms-events");
        h.meta("subms.recipe.category", "concurrency");
        h.writeJson(System.out);
    }
}
