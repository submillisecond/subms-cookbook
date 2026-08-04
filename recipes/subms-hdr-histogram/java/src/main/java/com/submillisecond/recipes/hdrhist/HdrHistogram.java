package com.submillisecond.recipes.hdrhist;

/**
 * Log-linear bucket histogram. Significant-digit precision in [1, 5].
 *
 * <p>A value's bucket is a major part (which doubling range it falls in) plus a
 * linear sub-part inside that range, so a bucket is never wider than
 * {@code 1 / subCount} of the value itself. {@code subCount} is
 * {@code 2 * 10^d} rounded up to a power of two, so {@code d = 3} gives
 * {@code 2^11 = 2048} sub-buckets and a worst-case quantisation error of
 * 1/2048 (0.049%), inside the half-unit-in-the-third-digit that three
 * significant digits demands.
 *
 * <p>The counter array starts at {@code subCount} entries and grows lazily to
 * cover the largest value recorded: at {@code d = 3} a range topping out at
 * 10^6 lands at index 20290 (~20k counters, 163 KB), and one topping out at
 * 10^9 at index 40678 (~41k counters, 326 KB).
 *
 * <p>Single-writer, single-reader. Use {@code ConcurrentHdrHistogram} when more
 * than one thread records.
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

    /**
     * Lowest value recorded, as its bucket's lower bound. 0 if empty. Read-side
     * sweep, same cost class as {@link #valueAtPercentile}.
     */
    public long min() {
        if (total == 0) return 0;
        final int end = Math.min(highIndex + 1, counters.length);
        for (int i = 0; i < end; i++) {
            if (counters[i] > 0) return valueFromIndex(i);
        }
        return 0;
    }

    /**
     * Arithmetic mean over the recorded bucket lower bounds. 0.0 if empty.
     * Quantised the same way the percentiles are, so it sits within the
     * significant-digit error band rather than being exact.
     */
    public double mean() {
        if (total == 0) return 0.0;
        double sum = 0.0;
        final int end = Math.min(highIndex + 1, counters.length);
        for (int i = 0; i < end; i++) {
            if (counters[i] > 0) sum += (double) counters[i] * (double) valueFromIndex(i);
        }
        return sum / (double) total;
    }

    /**
     * Recordings that landed in {@code value}'s bucket. Constant time - the
     * same index computation {@link #record} does.
     */
    public long countAtValue(long value) {
        int idx = indexOf(value);
        if (idx < 0 || idx >= counters.length) return 0;
        return counters[idx];
    }

    /**
     * Fraction of recordings at or below {@code value}'s bucket, in [0, 1]. The
     * inverse of {@link #valueAtPercentile}: that maps a rank to a value, this
     * maps a value to its rank.
     */
    public double percentileAtOrBelowValue(long value) {
        if (total == 0) return 0.0;
        int idx = indexOf(value);
        final int end = Math.min(idx + 1, Math.min(highIndex + 1, counters.length));
        long cum = 0;
        for (int i = 0; i < end; i++) cum += counters[i];
        return (double) cum / (double) total;
    }

    /**
     * Counter-array footprint in bytes. Grows with the largest value recorded,
     * not with how many values were recorded.
     */
    public long footprintBytes() {
        return (long) counters.length * Long.BYTES;
    }

    /**
     * Drop every recorded value. Keeps the array allocated, so a histogram
     * recycled across reporting intervals never re-enters the allocator.
     */
    public void reset() {
        java.util.Arrays.fill(counters, 0L);
        total = 0;
        highIndex = 0;
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

    // Internal surface, public only because the feature modules live in the
    // sibling `features` package and Java has no crate-private. The Rust port
    // keeps the same four pub(crate). `counters()` hands back the LIVE array so
    // the iterators can walk it without copying; writing through it desyncs
    // `total` and `highIndex`. Treat as read-only.
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
