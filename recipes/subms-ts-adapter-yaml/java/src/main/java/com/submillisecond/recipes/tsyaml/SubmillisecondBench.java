package com.submillisecond.recipes.tsyaml;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;

    // Only `encode` carries a per-op sub-ms gate, matching the Rust port. The
    // hand-written encode is a linear pass and holds p99 well under 1 ms. The
    // snakeyaml-backed decode allocates heavily; its p99 is GC-dominated and
    // volatile (sub-ms median, multi-ms tail), so the published decode claim is
    // throughput, not per-op latency, and the gate does not assert a decode p99
    // it cannot honestly hold. Decode numbers are captured in perf/java.json.
    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(20_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new YamlRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("encode", ONE_MS_NS)));
        System.out.println("OK (encode p99 under 1 ms)");
    }
}
