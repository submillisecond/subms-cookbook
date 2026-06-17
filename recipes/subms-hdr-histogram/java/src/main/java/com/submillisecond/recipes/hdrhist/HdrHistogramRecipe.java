package com.submillisecond.recipes.hdrhist;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class HdrHistogramRecipe implements SubMsRecipe {
    @Override public String name() { return "hdr-histogram"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        HdrHistogram hist = new HdrHistogram(3);

        Random r0 = new Random(seed);
        for (int i = 0; i < warmup; i++) hist.record(Math.abs(r0.nextLong()) + 1);

        SubMsPerfHarness.Stage rec = h.stage("record", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r1 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            long v = (Math.abs(r1.nextLong()) % 1_000_000L) + 1;
            long t0 = SubMsTimer.nanosNow();
            hist.record(v);
            rec.record(SubMsTimer.nanosNow() - t0);
        }

        // Warm the JIT: valueAtPercentile only runs 100 times in the
        // timed loop below, which is far short of C2's compilation
        // threshold (~10k calls). Without this warmup the measured
        // p99 reflects interpreted-mode execution; production hot-path
        // code runs C2-compiled and is ~50x faster.
        long warmSink = 0;
        for (int i = 0; i < 20_000; i++) {
            warmSink += hist.valueAtPercentile(0.99);
        }
        if (warmSink == Long.MIN_VALUE) System.out.println("warm");

        SubMsPerfHarness.Stage p = h.stage("percentile", 100).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < 100; i++) {
            long t0 = SubMsTimer.nanosNow();
            hist.valueAtPercentile(0.99);
            p.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("p99_ns", Long.toString(hist.valueAtPercentile(0.99)));
        h.meta("max_ns", Long.toString(hist.max()));
    }
}
