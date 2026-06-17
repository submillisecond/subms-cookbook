package com.submillisecond.primers.perfharness;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/**
 * The canonical recipe shape, end-to-end. Implements {@link SubMsRecipe} so
 * the entire pipeline becomes {@code SubMsBench.runBench(new HarnessRecipe(),
 * params)} - no bespoke driver code on the caller side.
 *
 * <p>Stages:
 * <ul>
 *   <li>{@code put}      - insert each key once, value derived deterministically</li>
 *   <li>{@code get_hit}  - look up every key, expect a hit</li>
 *   <li>{@code get_miss} - look up the same number of unseen keys</li>
 * </ul>
 *
 * <p>Each timed stage uses {@link SubMsPerfHarness.Stage#warmThenTime} so the
 * first samples land after C2 has compiled the hot path. Without this the p99
 * is dominated by interpreter-mode outliers in the first few thousand
 * iterations, and the bench measures the JIT instead of the structure.
 */
public final class HarnessRecipe implements SubMsRecipe {

    /** Differentiates the hit-key stream from the miss-key stream. */
    private static final long MISS_SALT = 0xDEAD_BEEF_CAFE_F00DL;

    @Override
    public String name() {
        return "subms-perf-harness";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup  = params.warmup();
        long seed   = params.seed();

        long[] hitKeys  = deterministicKeys(entries, seed);
        long[] missKeys = deterministicKeys(entries, seed ^ MISS_SALT);

        // First populate. Insert pass measures put latency on a fresh map.
        TinyMap putMap = new TinyMap(entries * 2);
        SubMsPerfHarness.Stage put = h.stage("put", entries);
        put.warmThenTime(warmup, entries, i -> {
            long k = hitKeys[i % entries];
            putMap.put(k, (int) k);
        });

        // Build a fully-populated fresh map for the read stages. The
        // put-stage map has the warmup keys overwritten and isn't a clean
        // baseline for hit/miss ratios.
        TinyMap readMap = new TinyMap(entries * 2);
        for (long k : hitKeys) readMap.put(k, (int) k);

        SubMsPerfHarness.Stage getHit = h.stage("get_hit", entries);
        getHit.warmThenTime(warmup, entries, i -> {
            readMap.get(hitKeys[i % entries]);
        });

        SubMsPerfHarness.Stage getMiss = h.stage("get_miss", entries);
        getMiss.warmThenTime(warmup, entries, i -> {
            readMap.containsKey(missKeys[i % entries]);
        });

        h.meta("hardware_tier", "laptop");
        h.meta("structure", "open-addressing-linear-probe");
        h.meta("load_factor_target", "0.75");
    }

    /** SplitMix64 over a counter. Cheap, deterministic, decent distribution.
     *  Avoids the EMPTY sentinel ({@link Long#MIN_VALUE}) the map rejects. */
    static long[] deterministicKeys(int n, long seed) {
        long[] out = new long[n];
        long x = seed == 0 ? 0x9E37_79B9_7F4A_7C15L : seed;
        for (int i = 0; i < n; i++) {
            x += 0x9E37_79B9_7F4A_7C15L;
            long z = x;
            z = (z ^ (z >>> 30)) * 0xBF58_476D_1CE4_E5B9L;
            z = (z ^ (z >>> 27)) * 0x94D0_49BB_1331_11EBL;
            z =  z ^ (z >>> 31);
            if (z == Long.MIN_VALUE) z = 1L;
            out[i] = z;
        }
        return out;
    }
}
