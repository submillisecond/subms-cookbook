package com.submillisecond.recipes.art;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class ArtRecipe implements SubMsRecipe {
    @Override public String name() { return "adaptive-radix-tree"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        Art<Integer> t = new Art<>();

        Random r0 = new Random(seed);
        for (int i = 0; i < warmup; i++) {
            t.insert(("k" + r0.nextInt()).getBytes(), 0);
        }

        SubMsPerfHarness.Stage ins = h.stage("insert", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r1 = new Random(seed + 1);
        String[] keys = new String[entries];
        for (int i = 0; i < entries; i++) {
            keys[i] = "k" + r1.nextInt();
            long t0 = SubMsTimer.nanosNow();
            t.insert(keys[i].getBytes(), 0);
            ins.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage get = h.stage("lookup", entries).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) {
            long t0 = SubMsTimer.nanosNow();
            t.get(k.getBytes());
            get.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("size", Integer.toString(t.size()));
    }
}
