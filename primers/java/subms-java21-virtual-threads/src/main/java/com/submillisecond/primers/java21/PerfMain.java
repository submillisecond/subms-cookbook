package com.submillisecond.primers.java21;

import java.io.IOException;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * stdin key=value params in, JSON on stdout. Drives {@link Java21Recipe}
 * through the cookbook harness.
 *
 * <pre>
 *   cat &lt;&lt;EOF | java -cp ... com.submillisecond.primers.java21.PerfMain
 *   entries=10000
 *   warmup=2000
 *   seed=0
 *   EOF
 * </pre>
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params = SubMsBenchParams.fromStdin();
        SubMsPerfHarness h = SubMsBench.runBench(new Java21Recipe(), params);
        h.writeJson(System.out);
    }
}
