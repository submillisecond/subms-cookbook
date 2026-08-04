package com.submillisecond.recipes.hdrhist.features;

/**
 * Frozen view of a {@link ConcurrentHdrHistogram} at the moment of
 * {@link ConcurrentHdrHistogram#drainSnapshot()}. Same percentile / max
 * API as the live histogram but immutable.
 */
public final class Snapshot {

    private final int subCountBits;
    private final long[] counts;
    private final long total;

    Snapshot(int subCountBits, long[] counts, long total) {
        this.subCountBits = subCountBits;
        this.counts = counts;
        this.total = total;
    }

    public long count() { return total; }

    public long max() {
        if (total == 0) return 0;
        for (int i = counts.length - 1; i >= 0; i--) {
            if (counts[i] > 0) return valueFromIndex(i);
        }
        return 0;
    }

    public long valueAtPercentile(double q) {
        if (total == 0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        long target = Math.max(1L, (long) (qc * total));
        long cum = 0;
        // `total` is the sum of `counts` by construction (drainSnapshot adds
        // each swapped counter), and target <= total, so the loop always
        // returns; the trailing 0 is only there to satisfy the compiler.
        for (int i = 0; i < counts.length; i++) {
            cum += counts[i];
            if (cum >= target) return valueFromIndex(i);
        }
        return 0;
    }

    private long valueFromIndex(int idx) {
        long subCnt = 1L << subCountBits;
        long subMask = subCnt - 1;
        long i = idx;
        if (i < subCnt) return i;
        long major = i >>> subCountBits;
        long sub = i & subMask;
        return (sub | subCnt) << (major - 1);
    }
}
