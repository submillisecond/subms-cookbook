package com.submillisecond.recipes.cuckoo;

import java.nio.charset.StandardCharsets;
import java.util.Random;

/**
 * Cuckoo filter. Bloom-alternative that supports delete.
 *
 * <p>Two candidate buckets per key. Each bucket holds {@code BUCKET_SIZE}
 * 8-bit fingerprints. Insert tries i1 then i2; if both full, kicks a random
 * fingerprint out and re-places it. Delete removes a matching fingerprint
 * from either bucket.
 */
public final class CuckooFilter {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;
    private static final int BUCKET_SIZE = 4;
    private static final int MAX_KICKS = 500;

    private final byte[][] buckets;
    private final int mask;
    private int count;
    private final Random rng = new Random(0xC0FFEE);

    public CuckooFilter(int expectedEntries) {
        int needed = (Math.max(1, expectedEntries) * 105 / 100) / BUCKET_SIZE + 1;
        int n = Math.max(2, needed);
        n = Integer.highestOneBit(n - 1) << 1; // round up to power of two
        this.buckets = new byte[n][BUCKET_SIZE];
        this.mask = n - 1;
    }

    public int size() { return count; }
    public boolean isEmpty() { return count == 0; }
    public int bucketCount() { return buckets.length; }

    /** Internal accessor used by the features/ siblings (snapshot, etc.). NOT part of the public API. */
    public byte[][] bucketsView() { return buckets; }

    /** Internal accessor used by the features/ siblings. NOT part of the public API. */
    public int maskView() { return mask; }

    public boolean insert(String key) {
        long h = mix(fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
        if (tryPlace(i1, fp) || tryPlace(i2, fp)) { count++; return true; }

        int bucketIdx = (rng.nextBoolean()) ? i1 : i2;
        byte victim = fp;
        for (int k = 0; k < MAX_KICKS; k++) {
            int slot = rng.nextInt(BUCKET_SIZE);
            byte tmp = buckets[bucketIdx][slot];
            buckets[bucketIdx][slot] = victim;
            victim = tmp;
            bucketIdx = (bucketIdx ^ altIndexOfFp(victim)) & mask;
            if (tryPlace(bucketIdx, victim)) { count++; return true; }
        }
        return false;
    }

    public boolean contains(String key) {
        long h = mix(fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
        return bucketHas(i1, fp) || bucketHas(i2, fp);
    }

    public boolean delete(String key) {
        long h = mix(fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
        if (bucketRemove(i1, fp) || bucketRemove(i2, fp)) { count--; return true; }
        return false;
    }

    private boolean tryPlace(int i, byte fp) {
        for (int s = 0; s < BUCKET_SIZE; s++) {
            if (buckets[i][s] == 0) {
                buckets[i][s] = fp;
                return true;
            }
        }
        return false;
    }

    private boolean bucketHas(int i, byte fp) {
        for (int s = 0; s < BUCKET_SIZE; s++) {
            if (buckets[i][s] == fp) return true;
        }
        return false;
    }

    private boolean bucketRemove(int i, byte fp) {
        for (int s = 0; s < BUCKET_SIZE; s++) {
            if (buckets[i][s] == fp) {
                buckets[i][s] = 0;
                return true;
            }
        }
        return false;
    }

    // Internal hash helpers reused by the features/ siblings. They
    // mirror the Rust crate-private helpers under `subms_cuckoo_filter`.
    // Marked public because the features subpackage cannot otherwise
    // reach them; NOT part of the supported API.

    /** Internal helper. NOT part of the public API. */
    public static int altIndexOfFp(byte fp) {
        return (int) ((fp & 0xffL) * 0x5bd1e9955L);
    }

    /** Internal helper. NOT part of the public API. */
    public static int altIndexOfFpWide(int fp) {
        return (int) ((fp & 0xffffL) * 0x5bd1e9955L);
    }

    /** Internal helper. NOT part of the public API. */
    public static long fnv1a64(byte[] bytes) {
        long h = FNV_OFFSET;
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= FNV_PRIME;
        }
        return h;
    }

    /** Internal helper. NOT part of the public API. */
    public static long mix(long h) {
        h ^= h >>> 30;
        h *= 0xbf58476d1ce4e5b9L;
        h ^= h >>> 27;
        h *= 0x94d049bb133111ebL;
        h ^= h >>> 31;
        return h;
    }

    /** Internal helper. NOT part of the public API. */
    public static int bucketSize() { return BUCKET_SIZE; }
}
