package com.submillisecond.recipes.cms;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

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

        SubMsPerfHarness.Stage add = h.stage("add", entries);
        Random r2 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            String key = "k" + r2.nextInt(1000);
            long t0 = System.nanoTime();
            cms.add(key);
            add.record(System.nanoTime() - t0);
        }

        SubMsPerfHarness.Stage est = h.stage("estimate", entries);
        Random r3 = new Random(seed + 2);
        for (int i = 0; i < entries; i++) {
            String key = "k" + r3.nextInt(1000);
            long t0 = System.nanoTime();
            cms.estimate(key);
            est.record(System.nanoTime() - t0);
        }

        h.meta("d", Integer.toString(cms.depth()));
        h.meta("w", Integer.toString(cms.width()));
    }
}
