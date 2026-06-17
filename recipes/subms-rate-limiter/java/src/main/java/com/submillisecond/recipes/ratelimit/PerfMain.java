package com.submillisecond.recipes.ratelimit;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new RateLimiterRecipe(), params);
        h.meta("subms.recipe.slug", "subms-rate-limiter");
        h.meta("subms.recipe.category", "scheduling");
        h.writeJson(System.out);
    }
}
