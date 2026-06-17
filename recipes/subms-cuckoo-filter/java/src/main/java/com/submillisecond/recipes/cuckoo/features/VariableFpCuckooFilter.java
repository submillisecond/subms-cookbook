package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.CuckooFilter;
import java.nio.charset.StandardCharsets;
import java.util.Random;

/**
 * Variable-width fingerprint cuckoo filter. The base filter pins
 * fingerprints to 8 bits (one byte per slot). This feature trades
 * memory for FPR by widening to 12 or 16 bits.
 *
 * <p>FPR scales as {@code 2 * b / 2^f}, where {@code b} = slots per
 * bucket and {@code f} = fingerprint width. At b=4: 8-bit ~3.1%,
 * 12-bit ~0.2%, 16-bit ~0.012%.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_cuckoo_filter::features::variable_fingerprint::VariableFpCuckooFilter}.
 */
public final class VariableFpCuckooFilter {

    public enum FingerprintWidth {
        EIGHT(8, 0x00ff),
        TWELVE(12, 0x0fff),
        SIXTEEN(16, 0xffff);

        private final int bits;
        private final int mask;

        FingerprintWidth(int bits, int mask) {
            this.bits = bits;
            this.mask = mask;
        }

        public int bits() { return bits; }
        int mask() { return mask; }
    }

    private static final int MAX_KICKS = 500;
    private static final int BUCKET_SIZE = 4;

    private final FingerprintWidth width;
    /** Buckets stored as short slots regardless of width, mirroring the Rust u16 layout. */
    private final short[][] buckets;
    private final int mask;
    private int count;
    private final Random rng = new Random(0xC0FFEE);

    public VariableFpCuckooFilter(int expectedEntries, FingerprintWidth width) {
        int needed = (Math.max(1, expectedEntries) * 105 / 100) / BUCKET_SIZE + 1;
        int n = Math.max(2, needed);
        n = Integer.highestOneBit(n - 1) << 1;
        this.width = width;
        this.buckets = new short[n][BUCKET_SIZE];
        this.mask = n - 1;
    }

    public FingerprintWidth width() { return width; }
    public int size() { return count; }
    public boolean isEmpty() { return count == 0; }
    public int bucketCount() { return buckets.length; }

    public boolean insert(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        int raw = (int) (h & width.mask());
        short fp = (short) (raw == 0 ? 1 : raw);
        int i1 = ((int) (h >>> 16)) & mask;
        int i2 = (i1 ^ altIndex(fp)) & mask;
        if (tryPlace(i1, fp) || tryPlace(i2, fp)) { count++; return true; }

        int bucketIdx = rng.nextBoolean() ? i1 : i2;
        short victim = fp;
        for (int k = 0; k < MAX_KICKS; k++) {
            int slot = rng.nextInt(BUCKET_SIZE);
            short tmp = buckets[bucketIdx][slot];
            buckets[bucketIdx][slot] = victim;
            victim = tmp;
            bucketIdx = (bucketIdx ^ altIndex(victim)) & mask;
            if (tryPlace(bucketIdx, victim)) { count++; return true; }
        }
        return false;
    }

    public boolean contains(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        int raw = (int) (h & width.mask());
        short fp = (short) (raw == 0 ? 1 : raw);
        int i1 = ((int) (h >>> 16)) & mask;
        int i2 = (i1 ^ altIndex(fp)) & mask;
        return bucketHas(i1, fp) || bucketHas(i2, fp);
    }

    public boolean delete(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        int raw = (int) (h & width.mask());
        short fp = (short) (raw == 0 ? 1 : raw);
        int i1 = ((int) (h >>> 16)) & mask;
        int i2 = (i1 ^ altIndex(fp)) & mask;
        if (bucketRemove(i1, fp) || bucketRemove(i2, fp)) { count--; return true; }
        return false;
    }

    private boolean tryPlace(int i, short fp) {
        for (int s = 0; s < BUCKET_SIZE; s++) {
            if (buckets[i][s] == 0) {
                buckets[i][s] = fp;
                return true;
            }
        }
        return false;
    }

    private boolean bucketHas(int i, short fp) {
        for (int s = 0; s < BUCKET_SIZE; s++) {
            if (buckets[i][s] == fp) return true;
        }
        return false;
    }

    private boolean bucketRemove(int i, short fp) {
        for (int s = 0; s < BUCKET_SIZE; s++) {
            if (buckets[i][s] == fp) {
                buckets[i][s] = 0;
                return true;
            }
        }
        return false;
    }

    private static int altIndex(short fp) {
        return CuckooFilter.altIndexOfFpWide(fp & 0xffff);
    }
}
