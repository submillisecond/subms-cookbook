package com.submillisecond.recipes.stats;

import java.util.List;
import java.util.Optional;

/**
 * Ergonomic wrapper around a {@code long[]} of nanosecond samples.
 * Method-chained API for the common statistics.
 *
 * <p>Use anywhere you have a buffer of timings (whether from
 * {@code SubMsPerfHarness} or your own measurement code) and want to
 * ask "what's the p99 / mean / CDF / jitter / tail" without dealing
 * with the static methods on {@link Percentiles} / {@link Histogram}
 * / etc. one at a time.
 *
 * <p>Construct with {@link #of(long[])}. The wrapper holds the
 * reference - it does NOT defensively copy. Callers wanting an
 * independent snapshot should clone the array first.
 */
public final class SubMsSamples {

    private final long[] raw;

    private SubMsSamples(long[] raw) {
        this.raw = raw;
    }

    /** Wrap a {@code long[]} of nanosecond samples. */
    public static SubMsSamples of(long[] raw) {
        return new SubMsSamples(raw == null ? new long[0] : raw);
    }

    public int count() { return raw.length; }
    public boolean isEmpty() { return raw.length == 0; }
    public long[] raw() { return raw; }

    public long p50() { return percentile(0.50); }
    public long p90() { return percentile(0.90); }
    public long p99() { return percentile(0.99); }
    public long p999() { return percentile(0.999); }

    public long max() {
        long m = 0;
        for (long v : raw) if (v > m) m = v;
        return m;
    }

    public long mean() { return Percentiles.mean(raw); }
    public long stddev() { return Percentiles.stddev(raw); }

    public long percentile(double q) {
        long[] sorted = raw.clone();
        java.util.Arrays.sort(sorted);
        return Percentiles.percentile(sorted, q);
    }

    public List<double[]> percentileSweep(double start, double end, double step) {
        return Percentiles.percentileSweep(raw, start, end, step);
    }

    public long[] cdfBuckets() { return Histogram.cdfBuckets(raw); }
    public double jitterScore() { return Jitter.jitterScore(raw); }

    public long conditionalTailExpectation(double q) { return Tail.conditionalTailExpectation(raw, q); }
    public double tailFatnessRatio() { return Tail.tailFatnessRatio(raw); }
    public Optional<Double> hillTailIndex(int k) { return Tail.hillTailIndex(raw, k); }

    public long iqr() { return Robust.iqr(raw); }
    public long medianAbsoluteDeviation() { return Robust.medianAbsoluteDeviation(raw); }
    public double coefficientOfVariation() { return Robust.coefficientOfVariation(raw); }
    public double skewness() { return Robust.skewness(raw); }
    public double kurtosis() { return Robust.kurtosis(raw); }

    public Bootstrap.CI bootstrapPercentileCi(double q, int iters, double confidence, long seed) {
        return Bootstrap.bootstrapPercentileCi(raw, q, iters, confidence, seed);
    }
}
