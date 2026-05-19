package com.submillisecond.recipes.hdrhist;

/**
 * Log-linear bucket histogram. Significant-digit precision in [1, 5].
 */
public final class HdrHistogram {

    private final int subCount;
    private final int subCountBits;
    private long[] counters;
    private long total;
    private int highIndex;

    public HdrHistogram(int significantDigits) {
        int sig = Math.min(5, Math.max(1, significantDigits));
        int target = 2 * (int) Math.pow(10, sig);
        int bits = Math.max(1, 32 - Integer.numberOfLeadingZeros(target));
        this.subCountBits = bits;
        this.subCount = 1 << bits;
        this.counters = new long[subCount];
    }

    public long count() { return total; }

    public long max() {
        if (total == 0) return 0;
        return valueFromIndex(highIndex);
    }

    public int subCount() { return subCount; }

    public void record(long value) {
        int idx = indexOf(value);
        if (idx >= counters.length) {
            long[] grown = new long[idx + 1];
            System.arraycopy(counters, 0, grown, 0, counters.length);
            counters = grown;
        }
        counters[idx]++;
        total++;
        if (idx > highIndex) highIndex = idx;
    }

    public long valueAtPercentile(double q) {
        if (total == 0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        long target = Math.max(1L, (long) (qc * total));
        long cum = 0;
        for (int i = 0; i < counters.length; i++) {
            cum += counters[i];
            if (cum >= target) return valueFromIndex(i);
        }
        return valueFromIndex(highIndex);
    }

    private int indexOf(long value) {
        long subMask = (1L << subCountBits) - 1;
        if (value <= subMask) return (int) value;
        int bits = 64 - Long.numberOfLeadingZeros(value);
        int major = bits - subCountBits;
        long sub = (value >>> (major - 1)) & subMask;
        return (major << subCountBits) | (int) sub;
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
