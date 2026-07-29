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

    /**
     * Record {@code value}, then correct for coordinated omission. Under a
     * fixed-rate load generator, one slow operation blocks every request that
     * should have been issued while it stalled; those requests are never sampled,
     * so the tail reads far better than the system delivered. When {@code value}
     * exceeds {@code expectedInterval}, this backfills the samples the generator
     * would have taken during the stall - synthetic values at
     * {@code value - expectedInterval}, {@code value - 2*expectedInterval}, ...
     * down to {@code expectedInterval} - so the percentiles reflect the latency
     * those blocked requests would have seen.
     *
     * <p>This is Gil Tene's {@code recordValueWithExpectedInterval}.
     * {@code expectedInterval == 0} (or a {@code value} no larger than it)
     * disables the correction, leaving this equivalent to {@link #record}.
     */
    public void recordWithExpectedInterval(long value, long expectedInterval) {
        record(value);
        if (expectedInterval <= 0 || value <= expectedInterval) return;
        for (long missing = value - expectedInterval;
                missing >= expectedInterval;
                missing -= expectedInterval) {
            record(missing);
        }
    }

    public long valueAtPercentile(double q) {
        if (total == 0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        long target = Math.max(1L, (long) (qc * total));
        long cum = 0;
        // Bound by highIndex - every counter past that is guaranteed
        // zero. `record()` can grow the array to a far-larger size than
        // the current data justifies (one record at idx=10000 leaves
        // the array at length 10001 even if every other record landed
        // in [0, 200]). Iterating the full length was the percentile-
        // p99=1.83ms outlier in the cookbook bench report.
        final long[] cs = counters;
        final int end = Math.min(highIndex + 1, cs.length);
        for (int i = 0; i < end; i++) {
            cum += cs[i];
            if (cum >= target) return valueFromIndex(i);
        }
        return valueFromIndex(highIndex);
    }

    // Bucket-index helpers shared with the feature modules.
    public int indexOf(long value) {
        long subMask = (1L << subCountBits) - 1;
        if (value <= subMask) return (int) value;
        int bits = 64 - Long.numberOfLeadingZeros(value);
        int major = bits - subCountBits;
        long sub = (value >>> (major - 1)) & subMask;
        return (major << subCountBits) | (int) sub;
    }

    public long valueFromIndex(int idx) {
        long subCnt = 1L << subCountBits;
        long subMask = subCnt - 1;
        long i = idx;
        if (i < subCnt) return i;
        long major = i >>> subCountBits;
        long sub = i & subMask;
        return (sub | subCnt) << (major - 1);
    }

    // Accessors used by the feature modules in
    // com.submillisecond.recipes.hdrhist.features. The arrays themselves
    // stay encapsulated; callers go through these.
    public int subCountBits() { return subCountBits; }
    public long[] counters() { return counters; }
    public int highIndex() { return highIndex; }

    /**
     * Sum another histogram's counters into this one. Errors if the two
     * histograms have different significant-digit shapes. Used by the
     * {@code merge} feature module.
     */
    public void addCountsFrom(HdrHistogram other) {
        if (this.subCountBits != other.subCountBits) {
            throw new IllegalArgumentException("significant-digit mismatch");
        }
        if (other.highIndex >= counters.length) {
            long[] grown = new long[other.highIndex + 1];
            System.arraycopy(counters, 0, grown, 0, counters.length);
            counters = grown;
        }
        for (int i = 0; i <= other.highIndex; i++) {
            long c = other.counters[i];
            if (c == 0L) continue;
            counters[i] += c;
            if (i > highIndex) highIndex = i;
        }
        total += other.total;
    }
}
