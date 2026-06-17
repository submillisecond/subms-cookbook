package com.submillisecond.recipes.segment;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new SegmentReaderRecipe(), params);
        h.meta("subms.recipe.slug", "subms-segment-reader");
        h.meta("subms.recipe.category", "storage");
        h.writeJson(System.out);
    }
}
