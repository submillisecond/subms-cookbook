package com.submillisecond.recipes.stats;

import java.util.Arrays;
import java.util.Optional;

/**
 * Tail analysis: the bit institutional users actually care about.
 *
 * <p>Byte-equivalent to {@code subms_stats::tail} in the Rust sibling.
 */
public final class Tail {
    private Tail() {}

    /**
     * Conditional tail expectation: the mean of all samples ABOVE the
     * given quantile. Also known as expected shortfall (ES) or
     * conditional value-at-risk (CVaR).
     *
     * <p>For latency: "given that we're in the worst N% of cases, what
     * does that average look like?"
     */
    public static long conditionalTailExpectation(long[] samples, double q) {
        if (samples.length == 0) return 0L;
        long[] sorted = samples.clone();
        Arrays.sort(sorted);
        long cutoff = Percentiles.percentile(sorted, q);
        long sum = 0;
        int count = 0;
        for (long v : sorted) {
            if (v > cutoff) {
                sum += v;
                count++;
            }
        }
        if (count == 0) return cutoff;
        return sum / count;
    }

    /**
     * Hill estimator for the tail index. Higher values mean a heavier
     * tail (more frequent extreme outliers). Uses the top {@code k}
     * order statistics. Returns {@link Optional#empty()} if
     * {@code k < 2} or fewer than {@code k+1} samples.
     */
    public static Optional<Double> hillTailIndex(long[] samples, int k) {
        if (k < 2 || samples.length <= k) return Optional.empty();
        long[] sorted = samples.clone();
        Arrays.sort(sorted);
        long pivot = sorted[sorted.length - k - 1];
        if (pivot == 0L) return Optional.empty();
        double pivotF = (double) pivot;
        double sum = 0;
        for (int i = 0; i < k; i++) {
            double v = (double) sorted[sorted.length - 1 - i];
            double ratio = v / pivotF;
            if (ratio > 0.0) sum += Math.log(ratio);
        }
        return Optional.of(sum / k);
    }

    /** Ratio of p99 over p50. {@code 0.0} when p50 is 0. */
    public static double tailFatnessRatio(long[] samples) {
        if (samples.length == 0) return 0.0;
        long[] sorted = samples.clone();
        Arrays.sort(sorted);
        long p50 = Percentiles.percentile(sorted, 0.50);
        long p99 = Percentiles.percentile(sorted, 0.99);
        if (p50 == 0L) return 0.0;
        return (double) p99 / (double) p50;
    }
}
