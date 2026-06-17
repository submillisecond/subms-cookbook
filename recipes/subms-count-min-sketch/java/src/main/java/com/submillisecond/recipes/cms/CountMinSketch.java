package com.submillisecond.recipes.cms;

import java.nio.charset.StandardCharsets;

/**
 * Count-Min Sketch with conservative update and Kirsch-Mitzenmacher hashing.
 *
 * <p>{@code d} rows of {@code w} counters; width is rounded up to a power of
 * two so indexing is a bitmask. Each insert finds the minimum cell across
 * the {@code d} rows and increments only those at the minimum. Query
 * returns the min cell. Always {@code estimate >= true_count}; over-estimation
 * is bounded by standard CMS analysis.
 */
public final class CountMinSketch {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

    private final int d;
    private final int w;
    private final int mask;
    private final int[][] rows;

    public CountMinSketch(int depth, int width) {
        int dd = Math.max(2, depth);
        int ww = Math.max(2, width);
        ww = Integer.highestOneBit(ww - 1) << 1; // round up to power of two
        this.d = dd;
        this.w = ww;
        this.mask = ww - 1;
        this.rows = new int[dd][ww];
    }

    public int depth() { return d; }
    public int width() { return w; }

    public void add(String key) {
        long[] hashes = baseHashes(key);
        long h1 = hashes[0], h2 = hashes[1];
        int min = Integer.MAX_VALUE;
        int[] idxs = new int[d];
        for (int i = 0; i < d; i++) {
            int idx = (int) (h1 + (long) i * h2) & mask;
            idxs[i] = idx;
            if (rows[i][idx] < min) min = rows[i][idx];
        }
        int next = min + 1;
        for (int i = 0; i < d; i++) {
            if (rows[i][idxs[i]] == min) rows[i][idxs[i]] = next;
        }
    }

    public int estimate(String key) {
        long[] hashes = baseHashes(key);
        long h1 = hashes[0], h2 = hashes[1];
        int min = Integer.MAX_VALUE;
        for (int i = 0; i < d; i++) {
            int idx = (int) (h1 + (long) i * h2) & mask;
            if (rows[i][idx] < min) min = rows[i][idx];
        }
        return min;
    }

    /**
     * Internal: in-place element-wise max-merge from {@code other}. Public
     * so the {@code features.Merge} class in the sub-package can call it.
     * Caller is responsible for validating shape; this method assumes
     * {@code other} has the same depth and width.
     *
     * @apiNote not part of the stable API surface.
     */
    public void applyPairedMax(CountMinSketch other) {
        for (int i = 0; i < d; i++) {
            int[] dst = rows[i];
            int[] src = other.rows[i];
            for (int j = 0; j < w; j++) {
                if (src[j] > dst[j]) dst[j] = src[j];
            }
        }
    }

    /**
     * Internal: zero every cell. Used by {@code features.WindowedCountMinSketch}
     * to recycle a slice on tick().
     *
     * @apiNote not part of the stable API surface.
     */
    public void clearAll() {
        for (int i = 0; i < d; i++) {
            int[] row = rows[i];
            for (int j = 0; j < w; j++) row[j] = 0;
        }
    }

    private static long[] baseHashes(String key) {
        long h = mix(fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        long h1 = h & 0xffffffffL;
        long h2 = ((h >>> 32) & 0xffffffffL) | 1L; // odd
        return new long[]{ h1, h2 };
    }

    private static long fnv1a64(byte[] bytes) {
        long h = FNV_OFFSET;
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= FNV_PRIME;
        }
        return h;
    }

    private static long mix(long h) {
        h ^= h >>> 30;
        h *= 0xbf58476d1ce4e5b9L;
        h ^= h >>> 27;
        h *= 0x94d049bb133111ebL;
        h ^= h >>> 31;
        return h;
    }
}
