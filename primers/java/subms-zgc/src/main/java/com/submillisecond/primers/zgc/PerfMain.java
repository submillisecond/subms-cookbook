package com.submillisecond.primers.zgc;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Run heartbeat-gap measurement through the cookbook harness. The GC under
 * test is the one the JVM was launched with (default G1; pass
 * {@code -XX:+UseZGC} or similar to compare).
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new ZgcRecipe(), params);
        h.writeJson(System.out);
    }
}
