package com.submillisecond.recipes.hll;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class HyperLogLogRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "hyperloglog";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        HyperLogLog hll = new HyperLogLog(14);

        Random r = new Random(seed);
        for (int i = 0; i < warmup; i++) hll.add("warm" + r.nextInt());

        SubMsPerfHarness.Stage add = h.stage("add", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r2 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            String key = "k" + r2.nextInt();
            long t0 = SubMsTimer.nanosNow();
            hll.add(key);
            add.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage est = h.stage("estimate", 100).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < 100; i++) {
            long t0 = SubMsTimer.nanosNow();
            hll.estimate();
            est.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("precision", "14");
        h.meta("registers", Integer.toString(hll.registerCount()));
        h.meta("estimate", Long.toString((long) hll.estimate()));
    }
}
