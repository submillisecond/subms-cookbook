package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.CuckooFilter;
import java.nio.charset.StandardCharsets;

/**
 * Immutable read-side snapshot of a cuckoo filter. The base filter is
 * single-writer; this feature lets readers fan out across threads
 * against a frozen {@code CuckooSnapshot} while a writer keeps
 * mutating the source filter independently.
 *
 * <p>Snapshots are eager copies, not lazy views: capture walks the
 * source's bucket array once and stores the result. That keeps
 * {@link #contains(String)} lock-free with no reader-side
 * coordination, at the cost of staleness vs the live writer.
 *
 * <p>Typical use: a hot-path reader cluster shares one
 * {@code CuckooSnapshot}; a background task periodically rebuilds the
 * snapshot from the live filter and publishes it via a volatile
 * reference. Readers see a consistent point-in-time view without
 * blocking writes.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_cuckoo_filter::features::concurrent_reads::CuckooSnapshot}.
 */
public final class CuckooSnapshot {

    private final byte[][] buckets;
    private final int mask;
    private final int count;

    private CuckooSnapshot(byte[][] buckets, int mask, int count) {
        this.buckets = buckets;
        this.mask = mask;
        this.count = count;
    }

    /** Capture a snapshot of {@code source}. Allocates a deep copy of the bucket array. */
    public static CuckooSnapshot capture(CuckooFilter source) {
        byte[][] src = source.bucketsView();
        byte[][] copy = new byte[src.length][];
        for (int i = 0; i < src.length; i++) {
            copy[i] = src[i].clone();
        }
        return new CuckooSnapshot(copy, source.maskView(), source.size());
    }

    public int size() { return count; }
    public boolean isEmpty() { return count == 0; }
    public int bucketCount() { return buckets.length; }

    public boolean contains(String key) {
        long h = CuckooFilter.mix(CuckooFilter.fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        byte fp = (byte) Math.max(1, (h & 0xff));
        int i1 = ((int) (h >>> 8)) & mask;
        int i2 = (i1 ^ CuckooFilter.altIndexOfFp(fp)) & mask;
        return bucketHas(i1, fp) || bucketHas(i2, fp);
    }

    /** Count non-empty fingerprint slots in the snapshot. */
    public int fingerprintCount() {
        int c = 0;
        for (byte[] b : buckets) {
            for (byte v : b) {
                if (v != 0) c++;
            }
        }
        return c;
    }

    private boolean bucketHas(int i, byte fp) {
        byte[] b = buckets[i];
        for (int s = 0; s < b.length; s++) {
            if (b[s] == fp) return true;
        }
        return false;
    }
}
