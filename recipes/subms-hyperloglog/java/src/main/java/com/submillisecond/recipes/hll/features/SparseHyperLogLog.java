package com.submillisecond.recipes.hll.features;

import com.submillisecond.recipes.hll.HllException;
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
 * cardinality. Sparse mode starts at zero payload and grows five bytes per
 * distinct register touched, until the dense array stops being an
 * over-allocation.
 *
 * <p>Crossover threshold defaults to {@code m / 4}, a round heuristic that
 * overshoots: at five bytes an entry the pair list reaches the dense array's
 * byte cost at {@code m/5}, so between {@code m/5} and {@code m/4} a sparse
 * sketch is both bigger and slower to probe. Pass an explicit
 * {@code m / 5} threshold when bytes are what you are buying. Promotion is
 * one-way.
 *
 * <p>This is the plain pair-list encoding, not HLL++'s. Heule et al. store the
 * sparse pairs at a higher temporary precision and difference-encode them as
 * varints behind a small unsorted temp set; that buys accuracy and bytes at
 * low cardinality and costs a merge step on every flush. Neither is
 * implemented here.
 *
 * <p><b>Thread safety.</b> Single-writer, same as the base type.
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
        this(precision, (1 << clampPrecision(precision)) / 4);
    }

    public SparseHyperLogLog(int precision, int threshold) {
        int pp = clampPrecision(precision);
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

    private static int clampPrecision(int precision) {
        return Math.min(HyperLogLog.MAX_PRECISION, Math.max(HyperLogLog.MIN_PRECISION, precision));
    }

    /** Wraps an already-dense sketch. Used by the wire codec; not stable API. */
    public static SparseHyperLogLog fromDense(HyperLogLog d) {
        SparseHyperLogLog out = new SparseHyperLogLog(d.precision());
        out.dense = d;
        out.sparseIdx = null;
        out.sparseRho = null;
        out.sparseLen = 0;
        return out;
    }

    /** Rebuilds a sparse sketch from a decoded entry list. Used by the codec; not stable API. */
    public static SparseHyperLogLog fromEntries(int precision, int threshold, int[] idx, byte[] rho) {
        SparseHyperLogLog out = new SparseHyperLogLog(precision, threshold);
        for (int i = 0; i < idx.length; i++) {
            if (out.sparseLen == out.sparseIdx.length) out.grow();
            out.sparseIdx[out.sparseLen] = idx[i];
            out.sparseRho[out.sparseLen] = rho[i];
            out.sparseLen++;
        }
        if (out.sparseLen >= out.threshold) out.promote();
        return out;
    }

    public int precision() { return p; }
    public int registerCount() { return m; }
    public boolean isSparse() { return dense == null; }

    /** Entry count at which this sketch promotes to dense. */
    public int threshold() { return threshold; }

    /**
     * Analytic relative standard error once dense, {@code 1.04 / sqrt(m)}.
     * Sparse mode is tighter than this because linear counting over a
     * mostly-empty register space is the accurate estimator down there; the
     * number is the envelope the sketch converges to, not a bound on its
     * current state.
     */
    public double standardError() {
        return HyperLogLog.RSE_CONSTANT / Math.sqrt(m);
    }

    /**
     * Live heap cost of the representation, register array or pair list. This
     * is the number the feature exists to move: at p=14 an untouched sparse
     * sketch is a fraction of the dense 16384 bytes.
     */
    public int stateBytes() {
        // 4 bytes of index plus 1 of rho per live entry, matching the Rust
        // port's (u32, u8) pair so the two ports report the same number.
        return dense != null ? dense.stateBytes() : sparseLen * 5;
    }

    /** True while nothing has been recorded. */
    public boolean isEmpty() {
        return dense != null ? dense.isEmpty() : sparseLen == 0;
    }

    /**
     * Reset to an empty sparse sketch, dropping the dense array if we had
     * promoted. The threshold survives; a reused sketch keeps its sizing.
     */
    public void clear() {
        this.dense = null;
        int cap = Math.max(8, threshold / 4);
        this.sparseIdx = new int[cap];
        this.sparseRho = new byte[cap];
        this.sparseLen = 0;
    }

    /** Distinct register entries currently held. */
    public int entryCount() {
        if (dense == null) return sparseLen;
        byte[] regs = dense.registers();
        int c = 0;
        for (byte r : regs) if (r != 0) c++;
        return c;
    }

    /**
     * Record a key. Returns true when the sketch changed. If we are sparse and
     * the new entry pushes us past the threshold, promote before returning.
     */
    public boolean add(String key) {
        return addBytes(key.getBytes(StandardCharsets.UTF_8));
    }

    /** Record a 64-bit id without rendering it to a string. */
    public boolean addLong(long key) {
        byte[] buf = new byte[8];
        for (int i = 0; i < 8; i++) {
            buf[i] = (byte) (key >>> (56 - 8 * i));
        }
        return addBytes(buf);
    }

    /** Record raw bytes. */
    public boolean addBytes(byte[] key) {
        if (dense != null) {
            return dense.addBytes(key);
        }
        long h = mix(fnv1a64(key));
        int idx = (int) (h >>> (64 - p));
        long w = (h << p) | (1L << (p - 1));
        int r = Long.numberOfLeadingZeros(w) + 1;

        // Linear-probe the sparse list.
        for (int i = 0; i < sparseLen; i++) {
            if (sparseIdx[i] == idx) {
                if ((byte) r > sparseRho[i]) {
                    sparseRho[i] = (byte) r;
                    return true;
                }
                return false;
            }
        }
        if (sparseLen == sparseIdx.length) grow();
        sparseIdx[sparseLen] = idx;
        sparseRho[sparseLen] = (byte) r;
        sparseLen++;
        if (sparseLen >= threshold) {
            promote();
        }
        return true;
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

    /**
     * Merge another sparse sketch of the same precision. Two sparse lists
     * combine entry-wise and may cross the threshold on the way, in which case
     * the result promotes. Once either side is dense the merge runs on dense
     * registers, which is where a fan-in of many shards ends up.
     */
    public void merge(SparseHyperLogLog other) {
        if (this.p != other.p) {
            throw HllException.precisionMismatch(this.p, other.p);
        }
        if (other.dense != null) {
            promote();
        }
        if (dense == null) {
            for (int i = 0; i < other.sparseLen; i++) {
                mergeEntry(other.sparseIdx[i], other.sparseRho[i]);
            }
            if (sparseLen >= threshold) promote();
            return;
        }
        if (other.dense != null) {
            dense.merge(other.dense);
        } else {
            int[] idx = new int[other.sparseLen];
            byte[] rho = new byte[other.sparseLen];
            System.arraycopy(other.sparseIdx, 0, idx, 0, other.sparseLen);
            System.arraycopy(other.sparseRho, 0, rho, 0, other.sparseLen);
            dense.applySparse(idx, rho);
        }
    }

    private void mergeEntry(int idx, byte r) {
        for (int i = 0; i < sparseLen; i++) {
            if (sparseIdx[i] == idx) {
                if (r > sparseRho[i]) sparseRho[i] = r;
                return;
            }
        }
        if (sparseLen == sparseIdx.length) grow();
        sparseIdx[sparseLen] = idx;
        sparseRho[sparseLen] = r;
        sparseLen++;
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

    /**
     * Materialise a dense copy without mutating this sketch. The bridge to
     * {@link UnionIntersect}, which only takes base sketches.
     */
    public HyperLogLog toDense() {
        if (dense != null) return dense.copy();
        HyperLogLog out = new HyperLogLog(p);
        int[]  idx = new int[sparseLen];
        byte[] rho = new byte[sparseLen];
        System.arraycopy(sparseIdx, 0, idx, 0, sparseLen);
        System.arraycopy(sparseRho, 0, rho, 0, sparseLen);
        out.applySparse(idx, rho);
        return out;
    }

    @Override
    public String toString() {
        return "SparseHyperLogLog{p=" + p + ", sparse=" + isSparse()
            + ", entries=" + entryCount() + ", estimate=" + estimate() + "}";
    }

    // ----- codec access ----------------------------------------------

    /** Live register indices. For the wire codec; not stable API. */
    public int[] entryIndices() {
        int[] out = new int[sparseLen];
        System.arraycopy(sparseIdx, 0, out, 0, sparseLen);
        return out;
    }

    /** Live rho values. For the wire codec; not stable API. */
    public byte[] entryRhos() {
        byte[] out = new byte[sparseLen];
        System.arraycopy(sparseRho, 0, out, 0, sparseLen);
        return out;
    }

    // ----- helpers ---------------------------------------------------

    private void grow() {
        int n = Math.min(sparseIdx.length * 2, threshold + 4);
        if (n <= sparseIdx.length) n = sparseIdx.length + 4;
        int[]  ni = new int[n];
        byte[] nr = new byte[n];
        System.arraycopy(sparseIdx, 0, ni, 0, sparseLen);
        System.arraycopy(sparseRho, 0, nr, 0, sparseLen);
        sparseIdx = ni;
        sparseRho = nr;
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
