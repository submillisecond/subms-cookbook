package com.submillisecond.recipes.arena;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new ArenaAllocatorRecipe(), params);
        h.meta("subms.recipe.slug", "subms-arena-allocator");
        h.meta("subms.recipe.category", "memory");
        h.writeJson(System.out);
    }
}
