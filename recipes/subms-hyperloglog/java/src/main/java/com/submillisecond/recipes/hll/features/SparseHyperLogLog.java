package com.submillisecond.recipes.hll.features;

import com.submillisecond.recipes.hll.HyperLogLog;
import java.nio.charset.StandardCharsets;

/**
 * Sparse HyperLogLog encoding for low-cardinality streams. Holds an
 * {@code (idx, rho)} pair list until entry count crosses a configured
 * threshold, then promotes itself into a dense {@link HyperLogLog}.
 *
 * <p>Why this exists: a p=14 HLL allocates 16 KB even when it has seen
 * zero items. Pipelines maintaining millions of sketches keyed by
 * tenant / customer / shard pay 16 KB times N regardless of actual
 * cardinality. Sparse mode starts at ~80 B and grows linearly until
 * the dense crossover.
 *
 * <p>Crossover threshold defaults to {@code m / 4}. Past that, the
 * sparse list is approaching dense's memory cost without dense's O(1)
 * lookup. Promotion is one-way.
 */
public final class SparseHyperLogLog {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

    private final int p;
    private final int m;
    private final double alpha;
    private final int threshold;

    /** Parallel arrays for the sparse list. Null after promotion. */
    private int[]  sparseIdx;
    private byte[] sparseRho;
    private int    sparseLen;

    /** Populated after promotion; null while sparse. */
    private HyperLogLog dense;

    public SparseHyperLogLog(int precision) {
        this(precision, (1 << Math.min(18, Math.max(4, precision))) / 4);
    }

    public SparseHyperLogLog(int precision, int threshold) {
        int pp = Math.min(18, Math.max(4, precision));
        this.p = pp;
        this.m = 1 << pp;
        this.alpha = HyperLogLog.alphaForRegisters(this.m);
        this.threshold = Math.max(1, threshold);
        // Initial capacity sized for a quarter of the threshold so we
        // don't repeatedly resize during burst inserts.
        int cap = Math.max(8, this.threshold / 4);
        this.sparseIdx = new int[cap];
        this.sparseRho = new byte[cap];
        this.sparseLen = 0;
    }

    public int precision() { return p; }
    public int registerCount() { return m; }
    public boolean isSparse() { return dense == null; }

    /** Distinct register entries currently held. */
    public int entryCount() {
        if (dense == null) return sparseLen;
        byte[] regs = registersOf(dense);
        int c = 0;
        for (byte r : regs) if (r != 0) c++;
        return c;
    }

    public void add(String key) {
        long h = mix(fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        int idx = (int) (h >>> (64 - p));
        long w = (h << p) | (1L << (p - 1));
        int r = Long.numberOfLeadingZeros(w) + 1;

        if (dense != null) {
            dense.add(key);
            return;
        }

        // Linear-probe the sparse list.
        for (int i = 0; i < sparseLen; i++) {
            if (sparseIdx[i] == idx) {
                if ((byte) r > sparseRho[i]) sparseRho[i] = (byte) r;
                return;
            }
        }
        if (sparseLen == sparseIdx.length) {
            int n = Math.min(sparseIdx.length * 2, threshold + 4);
            int[]  ni = new int[n];
            byte[] nr = new byte[n];
            System.arraycopy(sparseIdx, 0, ni, 0, sparseLen);
            System.arraycopy(sparseRho, 0, nr, 0, sparseLen);
            sparseIdx = ni;
            sparseRho = nr;
        }
        sparseIdx[sparseLen] = idx;
        sparseRho[sparseLen] = (byte) r;
        sparseLen++;
        if (sparseLen >= threshold) {
            promote();
        }
    }

    public double estimate() {
        if (dense != null) return dense.estimate();
        // Sum 2^-r over held entries; absent registers contribute 1
        // each (2^0). Same formula as the base HLL but specialised so
        // we don't materialise the m-byte register array.
        double held = 0.0;
        for (int i = 0; i < sparseLen; i++) {
            held += Math.pow(2.0, -(sparseRho[i] & 0xff));
        }
        int zeros = m - sparseLen;
        double sum = held + (double) zeros;
        double raw = alpha * (double) m * (double) m / sum;
        if (zeros > 0 && raw <= 2.5 * m) {
            return -((double) m) * Math.log((double) zeros / (double) m);
        }
        return raw;
    }

    /** Force promotion to dense even if below threshold. Idempotent. */
    public void promote() {
        if (dense != null) return;
        HyperLogLog d = new HyperLogLog(p);
        // Trim to exact length so applySparse doesn't read past the
        // last live entry.
        int[]  trimmedIdx = new int[sparseLen];
        byte[] trimmedRho = new byte[sparseLen];
        System.arraycopy(sparseIdx, 0, trimmedIdx, 0, sparseLen);
        System.arraycopy(sparseRho, 0, trimmedRho, 0, sparseLen);
        d.applySparse(trimmedIdx, trimmedRho);
        this.dense = d;
        this.sparseIdx = null;
        this.sparseRho = null;
        this.sparseLen = 0;
    }

    /** View of the dense HLL after promotion. Null while sparse. */
    public HyperLogLog asDense() { return dense; }

    // ----- helpers ---------------------------------------------------

    private static byte[] registersOf(HyperLogLog hll) {
        return hll.registers();
    }

    private static long fnv1a64(byte[] bytes) {
        long h = FNV_OFFSET;
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= FNV_PRIME;
        }
        return h;
    }

    private static long mix(long h) {
        h ^= h >>> 30;
        h *= 0xbf58476d1ce4e5b9L;
        h ^= h >>> 27;
        h *= 0x94d049bb133111ebL;
        h ^= h >>> 31;
        return h;
    }
}
