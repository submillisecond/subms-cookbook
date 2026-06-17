package com.submillisecond.recipes.bloom;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/**
 * Minimal bloom filter - standalone, reusable, zero-dependency.
 *
 * <p>Standard double-hashed bloom filter: FNV-1a 64-bit produces two 32-bit
 * subhashes for the double-hashing trick. Sizing defaults to ~10 bits per
 * key and k=7, which gives ~1% false-positive rate. Suitable as a building
 * block for other cookbook samples (LSM tree SSTables in particular).
 *
 * <p>The on-disk layout is fixed and language-agnostic:
 * <pre>
 *   bit_count: u32 big-endian
 *   k:         u32 big-endian
 *   words:     u32 big-endian - number of u64 words
 *   bits:      (u64 big-endian) * words
 * </pre>
 */
public final class BloomFilter {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

    private final int bitCount;
    private final int k;
    private final long[] bits;

    /** Build an empty filter sized for {@code expectedEntries} at ~1% FPR (10 bits/key, k=7). */
    public BloomFilter(int expectedEntries) {
        this.bitCount = Math.max(64, expectedEntries * 10);
        this.k = 7;
        this.bits = new long[(bitCount + 63) >>> 6];
    }

    private BloomFilter(int bitCount, int k, long[] bits) {
        this.bitCount = bitCount;
        this.k = k;
        this.bits = bits;
    }

    public void add(String key) {
        long h = fnv1a64(key);
        int h1 = (int) h;
        int h2 = ((int) (h >>> 32)) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Math.floorMod(h1 + i * h2, bitCount);
            bits[idx >>> 6] |= 1L << (idx & 63);
        }
    }

    public boolean mightContain(String key) {
        long h = fnv1a64(key);
        int h1 = (int) h;
        int h2 = ((int) (h >>> 32)) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Math.floorMod(h1 + i * h2, bitCount);
            if ((bits[idx >>> 6] & (1L << (idx & 63))) == 0) return false;
        }
        return true;
    }

    public int bitCount() {
        return bitCount;
    }

    public int k() {
        return k;
    }

    public void writeTo(DataOutputStream out) throws IOException {
        out.writeInt(bitCount);
        out.writeInt(k);
        out.writeInt(bits.length);
        for (long w : bits) out.writeLong(w);
    }

    /** Parse a serialised filter from a slice of {@code buf}. */
    public static BloomFilter parse(byte[] buf, int off, int len) throws IOException {
        try (DataInputStream in = new DataInputStream(new ByteArrayInputStream(buf, off, len))) {
            int bitCount = in.readInt();
            int k = in.readInt();
            int words = in.readInt();
            long[] bits = new long[words];
            for (int i = 0; i < words; i++) bits[i] = in.readLong();
            return new BloomFilter(bitCount, k, bits);
        }
    }

    /** Public so the {@code features.*} sub-package classes can reuse the
     *  hash (Java's package-private doesn't reach sub-packages). */
    public static long fnv1a64(String key) {
        long h = FNV_OFFSET;
        byte[] bytes = key.getBytes(StandardCharsets.UTF_8);
        for (byte b : bytes) {
            h ^= b & 0xffL;
            h *= FNV_PRIME;
        }
        return h;
    }
}
