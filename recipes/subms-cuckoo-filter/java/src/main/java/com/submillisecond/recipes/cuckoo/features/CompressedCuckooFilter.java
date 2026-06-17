package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.CuckooFilter;
import java.nio.charset.StandardCharsets;
import java.util.Random;

/**
 * Compressed-bucket cuckoo filter. Stores each bucket as a
 * variable-length run of sorted fingerprints with a 1-byte count
 * prefix, instead of the fixed 4-byte slot array of the base filter.
 *
 * <p>At 50% load (2 fps/bucket average) the on-disk footprint is ~3
 * bytes/bucket vs 4 for the base; at 95% load it's ~5 vs 4 (the base
 * wins at near-saturation). The win is the low-to-moderate load
 * regime, which is where most production cuckoo filters sit.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_cuckoo_filter::features::compressed_buckets::CompressedCuckooFilter}.
 */
public final class CompressedCuckooFilter {

    private static final int MAX_KICKS = 500;
    private static final int BUCKET_SIZE = 4;

    /** Per-bucket layout: index 0 is the count, indices 1..count are sorted fps. */
    private final byte[][] buckets;
    private final int mask;
    private int count;
    private final Random rng = new Random(0xC0FFEE);

    public CompressedCuckooFilter(int expectedEntries) {
        int needed = (Math.max(1, expectedEntries) * 105 / 100) / BUCKET_SIZE + 1;
        int n = Math.max(2, needed);
        n = Integer.highestOneBit(n - 1) << 1;
        this.buckets = new byte[n][BUCKET_SIZE + 1];
        this.mask = n - 1;
    }

    public int size() { return count; }
    public boolean isEmpty() { return count == 0; }
    public int bucketCount() { return buckets.length; }

    /** Byte cost of the live occupied state (the wire-format size). */
    public int occupiedBytes() {
        int sum = 0;
        for (byte[] b : buckets) {
            sum += 1 + (b[0] & 0xff);
        }
        return sum;
    }

    public boolean insert(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        if (tryPlace(i1, fp) || tryPlace(i2, fp)) { count++; return true; }

        int bucketIdx = rng.nextBoolean() ? i1 : i2;
        byte victim = fp;
        for (int k = 0; k < MAX_KICKS; k++) {
            int slot = rng.nextInt(BUCKET_SIZE);
            victim = swapAt(bucketIdx, slot, victim);
            bucketIdx = (bucketIdx ^ CuckooFilter.altIndexOfFp(victim)) & mask;
            if (tryPlace(bucketIdx, victim)) { count++; return true; }
        }
        return false;
    }

    public boolean contains(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        return bucketHas(i1, fp) || bucketHas(i2, fp);
    }

    public boolean delete(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        if (bucketRemove(i1, fp) || bucketRemove(i2, fp)) { count--; return true; }
        return false;
    }

    byte[][] bucketsForTest() { return buckets; }

    private boolean tryPlace(int i, byte fp) {
        int c = buckets[i][0] & 0xff;
        if (c >= BUCKET_SIZE) return false;
        int pos = 0;
        // Find insertion point. Compare as unsigned (fp is signed byte
        // in Java, but order-equivalence with Rust requires unsigned).
        while (pos < c && (buckets[i][1 + pos] & 0xff) < (fp & 0xff)) {
            pos++;
        }
        for (int k = c - 1; k >= pos; k--) {
            buckets[i][1 + k + 1] = buckets[i][1 + k];
        }
        buckets[i][1 + pos] = fp;
        buckets[i][0] = (byte) (c + 1);
        return true;
    }

    private boolean bucketHas(int i, byte fp) {
        int c = buckets[i][0] & 0xff;
        for (int k = 0; k < c; k++) {
            int cur = buckets[i][1 + k] & 0xff;
            int target = fp & 0xff;
            if (cur == target) return true;
            if (cur > target) return false;
        }
        return false;
    }

    private boolean bucketRemove(int i, byte fp) {
        int c = buckets[i][0] & 0xff;
        for (int k = 0; k < c; k++) {
            if (buckets[i][1 + k] == fp) {
                for (int j = k; j < c - 1; j++) {
                    buckets[i][1 + j] = buckets[i][1 + j + 1];
                }
                buckets[i][1 + c - 1] = 0;
                buckets[i][0] = (byte) (c - 1);
                return true;
            }
        }
        return false;
    }

    private byte swapAt(int i, int slot, byte fp) {
        int c = buckets[i][0] & 0xff;
        int s = Math.min(slot, Math.max(0, c - 1));
        byte victim = buckets[i][1 + s];
        for (int j = s; j < c - 1; j++) {
            buckets[i][1 + j] = buckets[i][1 + j + 1];
        }
        buckets[i][1 + c - 1] = 0;
        buckets[i][0] = (byte) (c - 1);
        tryPlace(i, fp);
        return victim;
    }
}
