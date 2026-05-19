package com.submillisecond.recipes.treap;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

public final class TreapRecipe implements SubMsRecipe {

    @Override public String name() { return "treap"; }

    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        Treap<Integer, Integer> t = new Treap<>(seed);

        Random r0 = new Random(seed);
        for (int i = 0; i < warmup; i++) {
            int k = r0.nextInt();
            t.insert(k, k);
        }

        SubMsPerfHarness.Stage ins = h.stage("insert", entries);
        Random r1 = new Random(seed + 1);
        int[] keys = new int[entries];
        for (int i = 0; i < entries; i++) {
            int k = r1.nextInt();
            keys[i] = k;
            long t0 = System.nanoTime();
            t.insert(k, k);
            ins.record(System.nanoTime() - t0);
        }

        SubMsPerfHarness.Stage get = h.stage("lookup", entries);
        for (int k : keys) {
            long t0 = System.nanoTime();
            t.get(k);
            get.record(System.nanoTime() - t0);
        }

        h.meta("size", Integer.toString(t.size()));
    }
}
