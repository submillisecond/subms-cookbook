package com.submillisecond.recipes.bloom;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

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
            int idx = Integer.remainderUnsigned(h1 + i * h2, bitCount);
            bits[idx >>> 6] |= 1L << (idx & 63);
        }
    }

    public boolean mightContain(String key) {
        long h = fnv1a64(key);
        int h1 = (int) h;
        int h2 = ((int) (h >>> 32)) | 1;
        for (int i = 0; i < k; i++) {
            int idx = Integer.remainderUnsigned(h1 + i * h2, bitCount);
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

    /**
     * Population count of the bit array. Walks every word, so keep it off the
     * hot path; it is the input to both saturation estimators below.
     */
    public long setBits() {
        long n = 0;
        for (long w : bits) n += Long.bitCount(w);
        return n;
    }

    /**
     * Swamidass-Baldi estimate of how many distinct keys were added:
     * {@code -(m/k) * ln(1 - X/m)} for {@code X} set bits. Diverges once the
     * array saturates, so a fully set filter reports {@link Long#MAX_VALUE}
     * rather than a number that reads as real.
     */
    public long approximateElementCount() {
        double m = bitCount;
        double x = setBits();
        if (x >= m) return Long.MAX_VALUE;
        return Math.round(-(m / k) * Math.log(1.0 - x / m));
    }

    /**
     * Current false-positive probability given actual occupancy:
     * {@code (X/m)^k}. This is the measured rate, not the design-point ~1%,
     * so it is what tells you the filter has outgrown its sizing.
     */
    public double estimatedFpp() {
        return Math.pow((double) setBits() / bitCount, k);
    }

    /**
     * Two filters can be unioned only if they agree on {@code m} and {@code k}
     * - the bit positions mean nothing otherwise.
     */
    public boolean isCompatible(BloomFilter other) {
        return bitCount == other.bitCount && k == other.k && bits.length == other.bits.length;
    }

    /**
     * OR another filter's bits into this one. The result is the filter you
     * would have built by adding both key sets to one array, which is what
     * makes a shard-per-producer build mergeable at fan-in.
     *
     * @throws IllegalArgumentException if the two filters were sized differently
     */
    public void union(BloomFilter other) {
        if (!isCompatible(other)) {
            throw new IllegalArgumentException(String.format(
                "incompatible bloom geometry: m=%d k=%d vs m=%d k=%d",
                bitCount, k, other.bitCount, other.k));
        }
        for (int i = 0; i < bits.length; i++) bits[i] |= other.bits[i];
    }

    /**
     * Zero the bits, keeping the allocation. A generation boundary that
     * rebuilds membership from a source of truth reuses the array instead of
     * dropping and re-allocating it.
     */
    public void clear() {
        Arrays.fill(bits, 0L);
    }

    public void writeTo(DataOutputStream out) throws IOException {
        out.writeInt(bitCount);
        out.writeInt(k);
        out.writeInt(bits.length);
        for (long w : bits) out.writeLong(w);
    }

    /**
     * Parse a serialised filter from a slice of {@code buf}. The word count is
     * checked against the slice before the array is allocated: a corrupt header
     * claiming 2^30 words would otherwise reserve 8 GB on its way to failing.
     */
    public static BloomFilter parse(byte[] buf, int off, int len) throws IOException {
        if (len < 12) throw new IOException("bloom section too short");
        try (DataInputStream in = new DataInputStream(new ByteArrayInputStream(buf, off, len))) {
            int bitCount = in.readInt();
            int k = in.readInt();
            int words = in.readInt();
            if (words < 0 || (long) len < 12L + (long) words * 8L) {
                throw new IOException("bloom section truncated");
            }
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
