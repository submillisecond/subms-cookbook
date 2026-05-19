package com.submillisecond.recipes.bloom;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/** Stages: {@code add}, {@code might_contain_hit}, {@code might_contain_miss}. */
public final class BloomFilterRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "bloom-filter";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();

        BloomFilter bf = new BloomFilter(entries);

        // Warm-up so icache fill / branch warm-up don't leak into steady state.
        for (int i = 0; i < warmup; i++) bf.add("warm" + i);

        SubMsPerfHarness.Stage add = h.stage("add", entries);
        for (int i = 0; i < entries; i++) {
            String key = "key" + i;
            long t0 = System.nanoTime();
            bf.add(key);
            add.record(System.nanoTime() - t0);
        }

        Random r1 = new Random(seed);
        SubMsPerfHarness.Stage hit = h.stage("might_contain_hit", entries);
        for (int i = 0; i < entries; i++) {
            String key = "key" + r1.nextInt(entries);
            long t0 = System.nanoTime();
            bf.mightContain(key);
            hit.record(System.nanoTime() - t0);
        }

        Random r2 = new Random(seed + 1);
        SubMsPerfHarness.Stage miss = h.stage("might_contain_miss", entries);
        for (int i = 0; i < entries; i++) {
            String key = "absent" + r2.nextInt(entries * 10);
            long t0 = System.nanoTime();
            bf.mightContain(key);
            miss.record(System.nanoTime() - t0);
        }

        h.meta("bits_per_key", "10");
        h.meta("k", "7");
    }
}
