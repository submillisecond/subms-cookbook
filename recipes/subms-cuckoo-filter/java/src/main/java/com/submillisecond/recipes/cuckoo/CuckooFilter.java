package com.submillisecond.recipes.cuckoo;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Random;

/**
 * Cuckoo filter. Bloom-alternative that supports delete.
 *
 * <p>Two candidate buckets per key. Each bucket holds {@code BUCKET_SIZE}
 * 8-bit fingerprints. Insert tries i1 then i2; if both full, kicks a random
 * fingerprint out and re-places it. Delete removes a matching fingerprint
 * from either bucket.
 *
 * <p>Single writer, no internal synchronisation. Concurrent mutation from two
 * threads corrupts the bucket array; a reader concurrent with a writer can see
 * a torn slot. For read fan-out take a {@code CuckooSnapshot}.
 */
public final class CuckooFilter {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;
    /** Slots per bucket. 4 gives ~95% load factor before inserts start failing. */
    public static final int BUCKET_SIZE = 4;
    /** Max kick-out attempts during a single insert. */
    public static final int MAX_KICKS = 500;
    /** Fingerprint bits per slot in the base filter. */
    public static final int FINGERPRINT_BITS = 8;

    private static final int HEADER_BYTES = 4 + 8 + 1 + 4;

    private final byte[][] buckets;
    private final int mask;
    private int count;
    private final Random rng = new Random(0xC0FFEE);

    /**
     * The one fingerprint the eviction chain could not re-home, held here
     * rather than dropped. Without it a saturating insert silently evicts an
     * already-present key and the no-false-negative guarantee breaks. Zero
     * means empty, matching the slot sentinel.
     */
    private byte victimFp;
    private int victimBucket;

    public CuckooFilter(int expectedEntries) {
        int needed = (Math.max(1, expectedEntries) * 105 / 100) / BUCKET_SIZE + 1;
        int n = Math.max(2, needed);
        n = Integer.highestOneBit(n - 1) << 1; // round up to power of two
        this.buckets = new byte[n][BUCKET_SIZE];
        this.mask = n - 1;
    }

    private CuckooFilter(byte[][] buckets, int count, byte victimFp, int victimBucket) {
        this.buckets = buckets;
        this.mask = buckets.length - 1;
        this.count = count;
        this.victimFp = victimFp;
        this.victimBucket = victimBucket;
    }

    public int size() { return count; }
    public boolean isEmpty() { return count == 0; }
    public int bucketCount() { return buckets.length; }

    /**
     * Total fingerprint slots. The filter refuses new keys somewhere below
     * this, around 95% occupancy at {@code BUCKET_SIZE = 4}.
     */
    public int capacity() { return buckets.length * BUCKET_SIZE; }

    /** Occupied fraction of {@link #capacity()}. */
    public double loadFactor() { return (double) count / capacity(); }

    /**
     * Bytes held by the bucket array. Excludes object headers and the handful
     * of scalar fields; this is the term that scales.
     */
    public long sizeInBytes() { return (long) buckets.length * BUCKET_SIZE; }

    /**
     * False-positive probability at the current occupancy:
     * {@code 1 - (1 - 2^-f)^(2 * b * alpha)} for {@code f} fingerprint bits,
     * {@code b} slots per bucket and load factor {@code alpha}. A query touches
     * {@code 2b} slots, each a {@code 2^-f} chance of a fingerprint collision.
     */
    public double estimatedFpp() {
        double alpha = loadFactor();
        if (alpha <= 0.0) return 0.0;
        double perSlot = 1.0 - Math.pow(2.0, -FINGERPRINT_BITS);
        return 1.0 - Math.pow(perSlot, 2.0 * BUCKET_SIZE * alpha);
    }

    /**
     * Zero every slot, keeping the allocation. A session boundary that rebuilds
     * membership from a source of truth reuses the array instead of dropping
     * and re-allocating it.
     */
    public void clear() {
        for (byte[] b : buckets) Arrays.fill(b, (byte) 0);
        count = 0;
        victimFp = 0;
        victimBucket = 0;
    }

    /** Internal accessor used by the features/ siblings (snapshot, etc.). NOT part of the public API. */
    public byte[][] bucketsView() { return buckets; }

    /** Internal accessor used by the features/ siblings. NOT part of the public API. */
    public int maskView() { return mask; }

    /** Internal accessor: the parked eviction victim's fingerprint, or 0. NOT part of the public API. */
    public byte victimFpView() { return victimFp; }

    /** Internal accessor: the parked eviction victim's bucket. NOT part of the public API. */
    public int victimBucketView() { return victimBucket; }

    public boolean insert(String key) {
        return insert(key.getBytes(StandardCharsets.UTF_8));
    }

    /**
     * Insert over raw bytes. Market-data keys are rarely {@code String} - an
     * order id is a long, a symbol is a fixed-width field off the wire - and
     * forcing a UTF-8 allocation to reach the filter would dominate the op.
     */
    public boolean insert(byte[] key) {
        long h = mix(fnv1a64(key));
        byte fp = fingerprint(h);
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
        return place(fp, i1, i2);
    }

    /**
     * Insert only if the key is not already present, returning {@code true}
     * when it was added. One probe instead of two for the dedup shape: a feed
     * handler asking "have I seen this sequence number" and recording it in the
     * same breath. A false positive suppresses a genuinely new key, which is
     * the trade a dedup window is making anyway.
     */
    public boolean insertIfAbsent(String key) {
        return insertIfAbsent(key.getBytes(StandardCharsets.UTF_8));
    }

    /** {@link #insertIfAbsent(String)} over raw bytes. */
    public boolean insertIfAbsent(byte[] key) {
        long h = mix(fnv1a64(key));
        byte fp = fingerprint(h);
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
        if (bucketHas(i1, fp) || bucketHas(i2, fp) || victimMatches(fp, i1, i2)) {
            return false;
        }
        return place(fp, i1, i2);
    }

    /** {@link #insert(String)} with a typed refusal instead of a bare boolean. */
    public void tryInsert(String key) {
        if (!insert(key)) {
            throw new CuckooException(CuckooException.Reason.NOT_ENOUGH_SPACE,
                "cuckoo filter is saturated");
        }
    }

    public boolean contains(String key) {
        return contains(key.getBytes(StandardCharsets.UTF_8));
    }

    /**
     * Probe membership. False positives possible; false negatives impossible -
     * every cuckoo move leaves a fingerprint in one of its two candidate
     * buckets, and the one fingerprint an oversubscribed insert cannot re-home
     * is held in the victim slot rather than dropped.
     */
    public boolean contains(byte[] key) {
        long h = mix(fnv1a64(key));
        byte fp = fingerprint(h);
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
        return bucketHas(i1, fp) || bucketHas(i2, fp) || victimMatches(fp, i1, i2);
    }

    public boolean delete(String key) {
        return delete(key.getBytes(StandardCharsets.UTF_8));
    }

    public boolean delete(byte[] key) {
        long h = mix(fnv1a64(key));
        byte fp = fingerprint(h);
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ altIndexOfFp(fp)) & mask;
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

    /**
     * Merge {@code other} into this filter. Both must have the same bucket
     * count: a fingerprint's home bucket is an index into a specific geometry,
     * so copying one across filters of different widths would land it somewhere
     * neither candidate bucket covers, which is a false negative.
     *
     * <p>Unlike a bloom filter's OR, this walks and re-places every fingerprint,
     * so it is O(N) in {@code other}'s capacity and can fail on saturation. A
     * failed merge leaves the fingerprints placed so far in place; rebuild from
     * the sources rather than retrying into the same filter.
     *
     * @throws CuckooException if the geometries differ or this filter fills up
     */
    public void union(CuckooFilter other) {
        if (buckets.length != other.buckets.length) {
            throw new CuckooException(CuckooException.Reason.GEOMETRY_MISMATCH,
                String.format("incompatible cuckoo geometry: %d buckets vs %d",
                    buckets.length, other.buckets.length));
        }
        for (int i = 0; i < other.buckets.length; i++) {
            for (int s = 0; s < BUCKET_SIZE; s++) {
                byte fp = other.buckets[i][s];
                if (fp != 0 && !place(fp, i, (i ^ altIndexOfFp(fp)) & mask)) {
                    throw new CuckooException(CuckooException.Reason.NOT_ENOUGH_SPACE,
                        "cuckoo filter is saturated");
                }
            }
        }
        if (other.victimFp != 0) {
            int i1 = other.victimBucket;
            int i2 = (i1 ^ altIndexOfFp(other.victimFp)) & mask;
            if (!place(other.victimFp, i1, i2)) {
                throw new CuckooException(CuckooException.Reason.NOT_ENOUGH_SPACE,
                    "cuckoo filter is saturated");
            }
        }
    }

    /**
     * Serialise to the cross-language wire format: bucket count, live count and
     * victim slot as big-endian headers, then the bucket bytes in index order.
     * The Rust port reads and writes the same bytes. The PRNG state is
     * deliberately not carried - it only picks which slot to evict, so a
     * reloaded filter answers every query identically.
     */
    public void writeTo(DataOutputStream out) throws IOException {
        out.writeInt(buckets.length);
        out.writeLong(count);
        out.writeByte(victimFp);
        out.writeInt(victimBucket);
        for (byte[] b : buckets) out.write(b);
    }

    /**
     * Parse a serialised filter from a slice of {@code buf}. The bucket count is
     * validated as a power of two before the array is allocated: the mask
     * arithmetic is only a modular reduction when it is, and a corrupt header
     * would otherwise index out of the array on the first probe.
     */
    public static CuckooFilter parse(byte[] buf, int off, int len) throws IOException {
        if (len < HEADER_BYTES) throw new IOException("cuckoo header too short");
        try (DataInputStream in = new DataInputStream(new ByteArrayInputStream(buf, off, len))) {
            int numBuckets = in.readInt();
            long liveCount = in.readLong();
            byte victimFp = in.readByte();
            int victimBucket = in.readInt();
            if (numBuckets < 2 || Integer.bitCount(numBuckets) != 1) {
                throw new IOException("cuckoo bucket count must be a power of two >= 2");
            }
            if ((long) len < HEADER_BYTES + (long) numBuckets * BUCKET_SIZE) {
                throw new IOException("cuckoo body truncated");
            }
            if (victimBucket < 0 || victimBucket >= numBuckets) {
                throw new IOException("cuckoo victim bucket out of range");
            }
            byte[][] buckets = new byte[numBuckets][BUCKET_SIZE];
            for (int i = 0; i < numBuckets; i++) in.readFully(buckets[i]);
            return new CuckooFilter(buckets, (int) liveCount, victimFp, victimBucket);
        }
    }

    private boolean place(byte fp, int i1, int i2) {
        if (tryPlace(i1, fp) || tryPlace(i2, fp)) { count++; return true; }
        if (victimFp != 0) return false;

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
        // The chain ran out of moves holding a fingerprint that is already part
        // of the set. Park it instead of dropping it.
        victimFp = victim;
        victimBucket = bucketIdx;
        count++;
        return true;
    }

    private boolean victimMatches(byte fp, int i1, int i2) {
        return victimFp == fp && (victimBucket == i1 || victimBucket == i2);
    }

    /**
     * A delete frees a slot, so the parked fingerprint may fit again. Try it on
     * the way out of every successful delete: leaving the victim set is what
     * turns the next insert into a refusal.
     */
    private void rehomeVictim() {
        if (victimFp == 0) return;
        int alt = (victimBucket ^ altIndexOfFp(victimFp)) & mask;
        if (tryPlace(victimBucket, victimFp) || tryPlace(alt, victimFp)) {
            victimFp = 0;
        }
    }

    private static byte fingerprint(long h) {
        return (byte) Math.max(1, (h & 0xff));
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
