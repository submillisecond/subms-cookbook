package com.submillisecond.recipes.cms;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class CountMinSketchRecipe implements SubMsRecipe {

    @Override
    public String name() { return "count-min-sketch"; }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        CountMinSketch cms = new CountMinSketch(5, 16384);

        Random r = new Random(seed);
        for (int i = 0; i < warmup; i++) cms.add("warm" + r.nextInt());

        SubMsPerfHarness.Stage add = h.stage("add", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r2 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            String key = "k" + r2.nextInt(1000);
            long t0 = SubMsTimer.nanosNow();
            cms.add(key);
            add.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage est = h.stage("estimate", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r3 = new Random(seed + 2);
        for (int i = 0; i < entries; i++) {
            String key = "k" + r3.nextInt(1000);
            long t0 = SubMsTimer.nanosNow();
            cms.estimate(key);
            est.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("d", Integer.toString(cms.depth()));
        h.meta("w", Integer.toString(cms.width()));
    }
}
