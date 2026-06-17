package com.submillisecond.stats;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Core percentiles + first-moment statistics. Always available; no
 * Maven module / feature gate.
 *
 * <p>All methods are static. {@link #percentile} expects a sorted
 * array; {@link #percentileSweep}, {@link #mean}, {@link #stddev}
 * accept unsorted input.
 *
 * <p>Byte-equivalent to {@code subms_stats::percentiles} in the Rust
 * sibling.
 */
public final class Percentiles {
    private Percentiles() {}

    /**
     * Percentile over a sorted ns array. Empty -> 0. Index is
     * {@code min(n-1, floor(q*n))} so {@code q=1.0} returns the max.
     *
     * <p>Caller MUST sort the array first.
     */
    public static long percentile(long[] sorted, double q) {
        if (sorted.length == 0) return 0L;
        int idx = Math.min(sorted.length - 1, (int) (q * sorted.length));
        return sorted[idx];
    }

    /**
     * Sweep percentiles across a uniform quantile range. Returns
     * {@code (quantile, ns)} pairs. {@code start}/{@code end} in
     * {@code [0.0, 1.0]}. Includes both endpoints.
     */
    public static List<double[]> percentileSweep(long[] samples, double start, double end, double step) {
        List<double[]> out = new ArrayList<>();
        if (samples.length == 0 || step <= 0.0) return out;
        long[] sorted = samples.clone();
        Arrays.sort(sorted);
        double q = start;
        while (q <= end + 1e-9) {
            out.add(new double[]{q, (double) percentile(sorted, q)});
            q += step;
        }
        return out;
    }

    /** Arithmetic mean. {@code 0} when empty. Integer rounding. */
    public static long mean(long[] samples) {
        if (samples.length == 0) return 0L;
        long sum = 0;
        for (long v : samples) sum += v;
        return sum / samples.length;
    }

    /**
     * Sample standard deviation (n-1 denominator). 0 when count < 2.
     * Uses double internally to avoid overflow on long sample arrays
     * with small ns values.
     */
    public static long stddev(long[] samples) {
        int n = samples.length;
        if (n < 2) return 0L;
        double m = mean(samples);
        double variance = 0.0;
        for (long v : samples) {
            double d = (double) v - m;
            variance += d * d;
        }
        variance /= (n - 1);
        return Math.round(Math.sqrt(variance));
    }
}
