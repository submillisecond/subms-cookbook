package com.submillisecond.stats;

import java.util.Arrays;

/**
 * Robust statistics: low sensitivity to outliers, useful when the raw
 * distribution has a heavy tail (which latency typically does).
 *
 * <p>Byte-equivalent to {@code subms_stats::robust} in the Rust
 * sibling.
 */
public final class Robust {
    private Robust() {}

    /** Interquartile range (p75 - p25). */
    public static long iqr(long[] samples) {
        if (samples.length == 0) return 0L;
        long[] sorted = samples.clone();
        Arrays.sort(sorted);
        long p25 = Percentiles.percentile(sorted, 0.25);
        long p75 = Percentiles.percentile(sorted, 0.75);
        return Math.max(0L, p75 - p25);
    }

    /** Median absolute deviation: the median of {@code |x - median(x)|}. */
    public static long medianAbsoluteDeviation(long[] samples) {
        if (samples.length == 0) return 0L;
        long[] sorted = samples.clone();
        Arrays.sort(sorted);
        long median = Percentiles.percentile(sorted, 0.50);
        long[] devs = new long[sorted.length];
        for (int i = 0; i < sorted.length; i++) {
            long v = sorted[i];
            devs[i] = (v > median) ? v - median : median - v;
        }
        Arrays.sort(devs);
        return Percentiles.percentile(devs, 0.50);
    }

    /** Coefficient of variation: stddev / mean. {@code 0.0} for zero-mean. */
    public static double coefficientOfVariation(long[] samples) {
        double m = Percentiles.mean(samples);
        if (m <= 0.0) return 0.0;
        return (double) Percentiles.stddev(samples) / m;
    }

    /**
     * Skewness (3rd standardised moment). Positive skew means a right
     * tail (typical for latency distributions). 0 with fewer than 3
     * samples.
     */
    public static double skewness(long[] samples) {
        int n = samples.length;
        if (n < 3) return 0.0;
        double m = Percentiles.mean(samples);
        double s2 = 0, s3 = 0;
        for (long v : samples) {
            double d = (double) v - m;
            s2 += d * d;
            s3 += d * d * d;
        }
        double variance = s2 / n;
        double std = Math.sqrt(variance);
        if (std <= 0.0) return 0.0;
        return (s3 / n) / Math.pow(std, 3);
    }

    /**
     * Excess kurtosis (4th standardised moment minus 3). Positive
     * excess kurtosis means heavier tails than a normal distribution.
     */
    public static double kurtosis(long[] samples) {
        int n = samples.length;
        if (n < 4) return 0.0;
        double m = Percentiles.mean(samples);
        double s2 = 0, s4 = 0;
        for (long v : samples) {
            double d = (double) v - m;
            s2 += d * d;
            s4 += d * d * d * d;
        }
        double variance = s2 / n;
        if (variance <= 0.0) return 0.0;
        return (s4 / n) / (variance * variance) - 3.0;
    }
}
