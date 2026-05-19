package com.submillisecond.recipes.hdrhist;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

public final class HdrHistogramRecipe implements SubMsRecipe {
    @Override public String name() { return "hdr-histogram"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        HdrHistogram hist = new HdrHistogram(3);

        Random r0 = new Random(seed);
        for (int i = 0; i < warmup; i++) hist.record(Math.abs(r0.nextLong()) + 1);

        SubMsPerfHarness.Stage rec = h.stage("record", entries);
        Random r1 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            long v = (Math.abs(r1.nextLong()) % 1_000_000L) + 1;
            long t0 = System.nanoTime();
            hist.record(v);
            rec.record(System.nanoTime() - t0);
        }

        SubMsPerfHarness.Stage p = h.stage("percentile", 100);
        for (int i = 0; i < 100; i++) {
            long t0 = System.nanoTime();
            hist.valueAtPercentile(0.99);
            p.record(System.nanoTime() - t0);
        }

        h.meta("p99_ns", Long.toString(hist.valueAtPercentile(0.99)));
        h.meta("max_ns", Long.toString(hist.max()));
    }
}
