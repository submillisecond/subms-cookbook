package com.submillisecond.recipes.bloom.features;

import com.submillisecond.recipes.bloom.BloomFilter;

/**
 * Counting bloom filter: supports {@link #remove(String)} by storing
 * 4-bit counters per cell instead of 1-bit flags. Cost: 4x memory vs
 * the base filter; gain: a real {@code remove()} operation that the
 * base can't support (since clearing a base bit can disturb other
 * keys).
 *
 * <p>Sized for ~1% FPR at 10 bits per key, k=7 (same defaults as the
 * base {@link BloomFilter}). Counter saturates at 15 to bound memory;
 * on saturation a {@code remove()} won't reduce the counter for that
 * cell (no false negatives - some cells just stay "stuck" at 15).
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_bloom_filter::CountingBloomFilter}.
 */
public final class CountingBloomFilter {

    private final int bitCount;
    private final int k;
    /** 4 bits per cell - two cells per byte. */
    private final byte[] cells;

    public CountingBloomFilter(int expectedEntries) {
        int bc = Math.max(64, expectedEntries * 10);
        this.bitCount = bc;
        this.k = 7;
        // 4 bits per cell -> bytes = (bitCount + 1) / 2 (round up).
        this.cells = new byte[(bc + 1) / 2];
    }

    public int bitCount() { return bitCount; }
    public int k() { return k; }

    /** Add a key. Increments the per-cell 4-bit counter at each of the
     *  {@code k} positions, saturating at 15. */
    public void add(String key) {
        long h = BloomFilter.fnv1a64(key);
        int h1 = (int) h;
        int h2 = (int) (h >>> 32) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Integer.remainderUnsigned(h1 + i * h2, bitCount);
            incr(idx);
        }
    }

    /** Probabilistic membership query. No false negatives. */
    public boolean mightContain(String key) {
        long h = BloomFilter.fnv1a64(key);
        int h1 = (int) h;
        int h2 = (int) (h >>> 32) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Integer.remainderUnsigned(h1 + i * h2, bitCount);
            if (read(idx) == 0) return false;
        }
        return true;
    }

    /** Remove a key. Decrements each of the {@code k} counters. Cells
     *  at the saturation value (15) stay put. */
    public void remove(String key) {
        long h = BloomFilter.fnv1a64(key);
        int h1 = (int) h;
        int h2 = (int) (h >>> 32) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Integer.remainderUnsigned(h1 + i * h2, bitCount);
            decr(idx);
        }
    }

    private int read(int idx) {
        byte b = cells[idx / 2];
        return (idx % 2 == 0) ? (b & 0x0f) : ((b >> 4) & 0x0f);
    }

    private void write(int idx, int value) {
        int i = idx / 2;
        int v = value & 0x0f;
        if (idx % 2 == 0) {
            cells[i] = (byte) ((cells[i] & 0xf0) | v);
        } else {
            cells[i] = (byte) ((cells[i] & 0x0f) | (v << 4));
        }
    }

    private void incr(int idx) {
        int cur = read(idx);
        if (cur < 15) write(idx, cur + 1);
    }

    private void decr(int idx) {
        int cur = read(idx);
        if (cur > 0 && cur < 15) write(idx, cur - 1);
    }
}
