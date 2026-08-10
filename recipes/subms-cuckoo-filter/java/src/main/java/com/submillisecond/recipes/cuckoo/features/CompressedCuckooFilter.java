package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.CuckooFilter;
import java.io.DataOutputStream;
import java.io.IOException;
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
    private static final int HEADER_BYTES = 4 + 8 + 1 + 4;

    /** Per-bucket layout: index 0 is the count, indices 1..count are sorted fps. */
    private final byte[][] buckets;
    private final int mask;
    private int count;
    private final Random rng = new Random(0xC0FFEE);

    /** Same parked-victim slot as the base filter. */
    private byte victimFp;
    private int victimBucket;

    public CompressedCuckooFilter(int expectedEntries) {
        int needed = (Math.max(1, expectedEntries) * 105 / 100) / BUCKET_SIZE + 1;
        int n = Math.max(2, needed);
        n = Integer.highestOneBit(n - 1) << 1;
        this.buckets = new byte[n][BUCKET_SIZE + 1];
        this.mask = n - 1;
    }

    private CompressedCuckooFilter(byte[][] buckets, int count, byte victimFp, int victimBucket) {
        this.buckets = buckets;
        this.mask = buckets.length - 1;
        this.count = count;
        this.victimFp = victimFp;
        this.victimBucket = victimBucket;
    }

    /**
     * Serialise in the compact form: the same 17-byte header as the base
     * filter, then each bucket as a count byte followed by exactly that many
     * fingerprints. The empty tail slots the base layout always writes are what
     * this feature exists to leave out, so the stream is {@link #occupiedBytes}
     * plus the header. Byte-identical to the Rust port's {@code write_to}.
     */
    public void writeTo(DataOutputStream out) throws IOException {
        out.writeInt(buckets.length);
        out.writeLong(count);
        out.writeByte(victimFp);
        out.writeInt(victimBucket);
        for (byte[] b : buckets) {
            out.write(b, 0, 1 + (b[0] & 0xff));
        }
    }

    /**
     * Parse a compact-form filter. Each bucket's count byte is validated before
     * its run is read, because a corrupt count would otherwise walk the cursor
     * off the end of the buffer.
     */
    public static CompressedCuckooFilter parse(byte[] buf, int off, int len) throws IOException {
        if (len < HEADER_BYTES) throw new IOException("compressed cuckoo header too short");
        int numBuckets = readInt(buf, off);
        long liveCount = readLong(buf, off + 4);
        byte victimFp = buf[off + 12];
        int victimBucket = readInt(buf, off + 13);
        if (numBuckets < 2 || Integer.bitCount(numBuckets) != 1) {
            throw new IOException("bucket count must be a power of two >= 2");
        }
        if (victimBucket < 0 || victimBucket >= numBuckets) {
            throw new IOException("victim bucket out of range");
        }
        byte[][] buckets = new byte[numBuckets][BUCKET_SIZE + 1];
        int cur = HEADER_BYTES;
        for (int i = 0; i < numBuckets; i++) {
            if (cur >= len) throw new IOException("compressed cuckoo body truncated");
            int n = buf[off + cur] & 0xff;
            if (n > BUCKET_SIZE || cur + 1 + n > len) {
                throw new IOException("compressed cuckoo bucket run out of range");
            }
            buckets[i][0] = (byte) n;
            System.arraycopy(buf, off + cur + 1, buckets[i], 1, n);
            cur += 1 + n;
        }
        return new CompressedCuckooFilter(buckets, (int) liveCount, victimFp, victimBucket);
    }

    private static int readInt(byte[] b, int i) {
        return ((b[i] & 0xff) << 24) | ((b[i + 1] & 0xff) << 16)
            | ((b[i + 2] & 0xff) << 8) | (b[i + 3] & 0xff);
    }

    private static long readLong(byte[] b, int i) {
        return ((long) readInt(b, i) << 32) | (readInt(b, i + 4) & 0xffffffffL);
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

    /** Zero every bucket, keeping the allocation. */
    public void clear() {
        for (byte[] b : buckets) java.util.Arrays.fill(b, (byte) 0);
        count = 0;
        victimFp = 0;
        victimBucket = 0;
    }

    public boolean insert(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        if (tryPlace(i1, fp) || tryPlace(i2, fp)) { count++; return true; }
        if (victimFp != 0) return false;

        int bucketIdx = rng.nextBoolean() ? i1 : i2;
        byte victim = fp;
        for (int k = 0; k < MAX_KICKS; k++) {
            int slot = rng.nextInt(BUCKET_SIZE);
            victim = swapAt(bucketIdx, slot, victim);
            bucketIdx = (bucketIdx ^ CuckooFilter.altIndexOfFp(victim)) & mask;
            if (tryPlace(bucketIdx, victim)) { count++; return true; }
        }
        victimFp = victim;
        victimBucket = bucketIdx;
        count++;
        return true;
    }

    public boolean contains(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        return bucketHas(i1, fp) || bucketHas(i2, fp) || victimMatches(fp, i1, i2);
    }

    public boolean delete(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        if (bucketRemove(i1, fp) || bucketRemove(i2, fp)) {
            count--;
            rehomeVictim();
            return true;
        }
        if (victimMatches(fp, i1, i2)) {
            victimFp = 0;
            count--;
            return true;
        }
        return false;
    }

    private boolean victimMatches(byte fp, int i1, int i2) {
        return victimFp == fp && (victimBucket == i1 || victimBucket == i2);
    }

    private void rehomeVictim() {
        if (victimFp == 0) return;
        int alt = (victimBucket ^ CuckooFilter.altIndexOfFp(victimFp)) & mask;
        if (tryPlace(victimBucket, victimFp) || tryPlace(alt, victimFp)) {
            victimFp = 0;
        }
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
