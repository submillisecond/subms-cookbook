package com.submillisecond.recipes.blockcache;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class BlockCacheRecipe implements SubMsRecipe {
    @Override public String name() { return "block-cache"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        int cap = 1024;
        BlockCache<Integer, Integer> c = new BlockCache<>(cap);

        Random r0 = new Random(seed);
        for (int i = 0; i < warmup; i++) {
            int k = r0.nextInt(cap * 2);
            c.put(k, k);
        }

        SubMsPerfHarness.Stage get = h.stage("get", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r1 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            int k = r1.nextInt(cap * 2);
            long t0 = SubMsTimer.nanosNow();
            c.get(k);
            get.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage put = h.stage("put", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r2 = new Random(seed + 2);
        for (int i = 0; i < entries; i++) {
            int k = r2.nextInt(cap * 4);
            long t0 = SubMsTimer.nanosNow();
            c.put(k, k);
            put.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("capacity", Integer.toString(cap));
    }
}
