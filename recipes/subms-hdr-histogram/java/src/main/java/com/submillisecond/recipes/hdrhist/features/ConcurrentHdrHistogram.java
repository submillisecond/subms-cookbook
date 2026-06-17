package com.submillisecond.recipes.hdrhist.features;

import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicLongArray;

/**
 * Lock-free concurrent HDR histogram.
 *
 * <p>Same log-linear bucketing as the base {@code HdrHistogram} but
 * every counter is an atomic 64-bit slot. Producers call
 * {@link #record(long)} from any thread without external
 * synchronisation. Snapshot reads walk the array with relaxed loads;
 * point-in-time across writers, but each individual counter is intact.
 *
 * <p>Counter array is fixed-size and pre-allocated at construction.
 * A growable layout would need a lock around the resize, defeating
 * the lock-free property. Pick the upper bound (number of major
 * buckets) at construction - default 32 majors covers ~10^9 at
 * 3 sig-digits.
 */
public final class ConcurrentHdrHistogram {

    private final int subCount;
    private final int subCountBits;
    private final AtomicLongArray counters;
    private final AtomicLong total = new AtomicLong(0);
    /** Highest bucket index that has been written to. */
    private final AtomicLong highIndex = new AtomicLong(0);

    /** Construct with default capacity of 32 major buckets. */
    public ConcurrentHdrHistogram(int significantDigits) {
        this(significantDigits, 32);
    }

    /**
     * Explicit major-bucket capacity. Counter array length is
     * {@code subCount * majors}. Values that map past the last
     * bucket are clamped into the final bucket.
     */
    public ConcurrentHdrHistogram(int significantDigits, int majors) {
        int sig = Math.min(5, Math.max(1, significantDigits));
        int target = 2 * (int) Math.pow(10, sig);
        int bits = Math.max(1, 32 - Integer.numberOfLeadingZeros(target));
        this.subCountBits = bits;
        this.subCount = 1 << bits;
        int m = Math.max(1, majors);
        this.counters = new AtomicLongArray(subCount * m);
    }

    public int subCount() { return subCount; }

    int subCountBits() { return subCountBits; }

    public long count() { return total.get(); }

    public long max() {
        if (count() == 0) return 0;
        return valueFromIndex((int) highIndex.get());
    }

    /** Record a value. Safe from any thread. */
    public void record(long value) {
        int raw = indexOf(value);
        int idx = Math.min(raw, counters.length() - 1);
        counters.incrementAndGet(idx);
        total.incrementAndGet();
        // Race on highIndex is benign: any thread that wrote a
        // higher index will eventually win the CAS.
        long cur = highIndex.get();
        while (idx > cur) {
            if (highIndex.compareAndSet(cur, idx)) break;
            cur = highIndex.get();
        }
    }

    /** Snapshot-style percentile read against current counter values. */
    public long valueAtPercentile(double q) {
        long t = total.get();
        if (t == 0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        long target = Math.max(1L, (long) (qc * t));
        int high = (int) highIndex.get();
        long cum = 0;
        int end = Math.min(high + 1, counters.length());
        for (int i = 0; i < end; i++) {
            cum += counters.get(i);
            if (cum >= target) return valueFromIndex(i);
        }
        return valueFromIndex(high);
    }

    /**
     * Atomically drain every counter and total into a {@link Snapshot},
     * leaving the histogram empty. Used by {@link DualRecorder} to
     * harvest the inactive side. Per-counter swap is independent, so
     * concurrent producers' increments may land in EITHER the drained
     * snapshot or the now-zeroed live histogram - never double-counted
     * or lost.
     */
    public Snapshot drainSnapshot() {
        int high = (int) highIndex.get();
        int end = Math.min(high + 1, counters.length());
        long[] drained = new long[end];
        long drainedTotal = 0;
        for (int i = 0; i < end; i++) {
            long v = counters.getAndSet(i, 0L);
            drained[i] = v;
            drainedTotal += v;
        }
        total.addAndGet(-drainedTotal);
        highIndex.set(0);
        return new Snapshot(subCountBits, drained, drainedTotal);
    }

    // ----- bucket math (mirrors the base) -----

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
