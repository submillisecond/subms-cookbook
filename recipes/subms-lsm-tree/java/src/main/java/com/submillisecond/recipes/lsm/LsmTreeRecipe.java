package com.submillisecond.recipes.lsm;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/** Stages: {@code put}, {@code get_hit}, {@code get_miss}. Opens a fresh tree in a temp dir. */
public final class LsmTreeRecipe implements SubMsRecipe {

    private final int flushThresholdBytes;
    private final BloomMode bloomMode;

    public LsmTreeRecipe(int flushThresholdBytes, BloomMode bloomMode) {
        this.flushThresholdBytes = flushThresholdBytes;
        this.bloomMode = bloomMode;
    }

    @Override
    public String name() {
        return "lsm-tree";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();

        h.input("flush_threshold_bytes", Integer.toString(flushThresholdBytes));
        h.input("bloom_mode", bloomMode == BloomMode.ON ? "on" : "off");

        Path dir;
        try {
            dir = Files.createTempDirectory("lsm-recipe-");
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }

        int sstables;
        try (LsmTree lsm = new LsmTree(dir, flushThresholdBytes, bloomMode)) {
            for (int i = 0; i < warmup; i++) lsm.put("warm" + i, "v" + i);
            for (int i = 0; i < warmup; i++) lsm.get("warm" + i);

            SubMsPerfHarness.Stage put = h.stage("put", entries);
            for (int i = 0; i < entries; i++) {
                long t0 = System.nanoTime();
                lsm.put("key" + i, "v" + i);
                put.record(System.nanoTime() - t0);
            }

            Random r1 = new Random(seed);
            SubMsPerfHarness.Stage hit = h.stage("get_hit", entries);
            for (int i = 0; i < entries; i++) {
                String key = "key" + r1.nextInt(entries);
                long t0 = System.nanoTime();
                lsm.get(key);
                hit.record(System.nanoTime() - t0);
            }

            Random r2 = new Random(seed + 1);
            SubMsPerfHarness.Stage miss = h.stage("get_miss", entries);
            for (int i = 0; i < entries; i++) {
                String key = "missing" + r2.nextInt(entries * 10);
                long t0 = System.nanoTime();
                lsm.get(key);
                miss.record(System.nanoTime() - t0);
            }

            sstables = lsm.sstableCount();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        h.meta("sstables", Integer.toString(sstables));

        deleteRecursive(dir);
    }

    private static void deleteRecursive(Path dir) {
        try (var stream = Files.walk(dir)) {
            stream.sorted(Comparator.reverseOrder()).forEach(p -> {
                try { Files.deleteIfExists(p); } catch (IOException ignored) {}
            });
        } catch (IOException ignored) {}
    }
}
