package com.submillisecond.recipes.hdrhist.features;

/**
 * Exponentially-decaying histogram.
 *
 * <p>Each bucket carries an effective count that decays toward zero
 * over time. On read, counts are multiplied by
 * {@code e^(-dt / halflife * ln 2)} so the distribution reflects
 * recent activity more strongly than ancient activity.
 *
 * <p>Storage: a running {@code lastDecayNs} timestamp plus a {@code double[]}
 * counter array. {@code record()} brings the array up to date before
 * incrementing the target bucket, so new writes compete fairly with
 * the older, partly-decayed entries. Read paths apply the
 * accumulated decay factor lazily without mutating.
 *
 * <p>Time source is the {@link Clock} interface; tests pass
 * {@link ManualClock}, production passes a wall-clock impl.
 */
public final class DecayingHdrHistogram {

    private final int subCountBits;
    private double[] counters;
    private int highIndex;
    private long lastDecayNs;
    private final long halflifeNs;
    private final Clock clock;

    public DecayingHdrHistogram(int significantDigits, long halflifeNs, Clock clock) {
        int sig = Math.min(5, Math.max(1, significantDigits));
        int target = 2 * (int) Math.pow(10, sig);
        int bits = Math.max(1, 32 - Integer.numberOfLeadingZeros(target));
        this.subCountBits = bits;
        int subCount = 1 << bits;
        this.counters = new double[subCount];
        this.halflifeNs = Math.max(1L, halflifeNs);
        this.clock = clock;
        this.lastDecayNs = clock.nowNs();
    }

    /** Record a value. Decays the existing array to "now" first. */
    public void record(long value) {
        decayToNow();
        int idx = indexOf(value);
        if (idx >= counters.length) {
            double[] grown = new double[idx + 1];
            System.arraycopy(counters, 0, grown, 0, counters.length);
            counters = grown;
        }
        counters[idx] += 1.0;
        if (idx > highIndex) highIndex = idx;
    }

    /** Total effective count across all buckets. */
    public double count() {
        double factor = peekFactor();
        double sum = 0.0;
        for (double c : counters) sum += c;
        return sum * factor;
    }

    public long max() {
        int end = Math.min(highIndex, counters.length - 1);
        for (int i = end; i >= 0; i--) {
            if (counters[i] > 1e-9) return valueFromIndex(i);
        }
        return 0;
    }

    public long valueAtPercentile(double q) {
        double total = count();
        if (total <= 0.0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        double target = Math.max(Double.MIN_VALUE, qc * total);
        double factor = peekFactor();
        double cum = 0.0;
        int end = Math.min(highIndex + 1, counters.length);
        for (int i = 0; i < end; i++) {
            cum += counters[i] * factor;
            if (cum >= target) return valueFromIndex(i);
        }
        return valueFromIndex(highIndex);
    }

    public long halflifeNs() { return halflifeNs; }

    private double peekFactor() {
        long now = clock.nowNs();
        long dt = Math.max(0L, now - lastDecayNs);
        if (dt == 0L) return 1.0;
        return Math.exp(-((double) dt / (double) halflifeNs) * Math.log(2.0));
    }

    private void decayToNow() {
        double factor = peekFactor();
        if (factor < 1.0) {
            for (int i = 0; i < counters.length; i++) counters[i] *= factor;
            lastDecayNs = clock.nowNs();
        } else if (factor == 1.0 && lastDecayNs == 0L) {
            lastDecayNs = clock.nowNs();
        }
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
