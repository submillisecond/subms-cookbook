package com.submillisecond.recipes.bloom.features;

import java.util.Arrays;

import com.submillisecond.recipes.bloom.BloomFilter;

/**
 * Partitioned bloom filter: {@code k} independent slices of
 * {@code m/k} bits each. Hash function {@code i} writes/reads only
 * into slice {@code i}, so the filter behaves like {@code k} parallel
 * 1-hash filters AND'd together.
 *
 * <p>Why bother? Two reasons:
 * <ol>
 *   <li>The independent-slice model gives cleaner FPR math:
 *       cumulative {@code P(false positive) = (1 - (1 - 1/(m/k))^n)^k}.</li>
 *   <li>Each slice can be updated by an independent producer without
 *       coordination, via {@link #addToSlice}.</li>
 * </ol>
 *
 * <p>Same default sizing as the base filter: ~10 bits/key, k=7.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_bloom_filter::PartitionedBloomFilter}.
 */
public final class PartitionedBloomFilter {

    /** Bits per slice (m/k). */
    private final int sliceBits;
    private final int k;
    /** k slices, each {@code sliceBits} long, packed into long[] words. */
    private final long[][] slices;

    public PartitionedBloomFilter(int expectedEntries) {
        int bitCount = Math.max(64, expectedEntries * 10);
        this.k = 7;
        this.sliceBits = (bitCount + k - 1) / k;
        int words = (sliceBits + 63) / 64;
        this.slices = new long[k][];
        for (int i = 0; i < k; i++) slices[i] = new long[words];
    }

    public int sliceBits() { return sliceBits; }
    public int k() { return k; }
    public int bitCount() { return sliceBits * k; }

    /** Add a key. Each hash position writes into its own slice. */
    public void add(String key) {
        long h = BloomFilter.fnv1a64(key);
        int h1 = (int) h;
        int h2 = (int) (h >>> 32) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Integer.remainderUnsigned(h1 + i * h2, sliceBits);
            slices[i][idx / 64] |= 1L << (idx % 64);
        }
    }

    public boolean mightContain(String key) {
        long h = BloomFilter.fnv1a64(key);
        int h1 = (int) h;
        int h2 = (int) (h >>> 32) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Integer.remainderUnsigned(h1 + i * h2, sliceBits);
            if ((slices[i][idx / 64] & (1L << (idx % 64))) == 0L) return false;
        }
        return true;
    }

    /** Zero every slice, keeping the allocations. */
    public void clear() {
        for (long[] slice : slices) Arrays.fill(slice, 0L);
    }

    /** Update only slice {@code i} for the given key. Lets a producer
     *  that owns hash {@code i} add a key without coordinating with
     *  producers that own other slices.
     *
     *  @throws IndexOutOfBoundsException if {@code slice} is out of range. */
    public void addToSlice(String key, int slice) {
        if (slice < 0 || slice >= k) {
            throw new IndexOutOfBoundsException("slice index out of range: " + slice);
        }
        long h = BloomFilter.fnv1a64(key);
        int h1 = (int) h;
        int h2 = (int) (h >>> 32) | 1;
        int idx = Integer.remainderUnsigned(h1 + slice * h2, sliceBits);
        slices[slice][idx / 64] |= 1L << (idx % 64);
    }
}
