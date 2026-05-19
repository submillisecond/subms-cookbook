package com.submillisecond.recipes.lsm;

import java.io.IOException;
import java.util.Map;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * stdin key=value params in, JSON on stdout.
 *
 * <pre>
 *   cat &lt;&lt;EOF | java -cp ... com.submillisecond.recipes.lsm.PerfMain
 *   entries=50000
 *   flush_threshold_bytes=16000
 *   warmup=5000
 *   bloom_mode=on
 *   seed=0
 *   EOF
 * </pre>
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        Map<String, String> raw = SubMsPerfHarness.readStdinKv();
        SubMsBenchParams params = SubMsBenchParams.fromMap(raw);
        int flushThreshold = SubMsBenchParams.parseInt(raw, "flush_threshold_bytes", 16_000);
        boolean bloomOn = SubMsBenchParams.parseBool(raw, "bloom_mode", true);
        BloomMode mode = bloomOn ? BloomMode.ON : BloomMode.OFF;

        LsmTreeRecipe recipe = new LsmTreeRecipe(flushThreshold, mode);
        SubMsPerfHarness h = SubMsBench.runBench(recipe, params);
        h.writeJson(System.out);
    }
}
