package com.submillisecond.stats;

import java.util.Arrays;
import java.util.Optional;

/**
 * Distribution comparison: "is candidate slower than baseline?"
 *
 * <p>Byte-equivalent to {@code subms_stats::compare} in the Rust
 * sibling.
 */
public final class Compare {
    private Compare() {}

    /**
     * Two-sample Kolmogorov-Smirnov D statistic: the maximum vertical
     * gap between the two empirical CDFs. Range {@code [0.0, 1.0]};
     * larger means more different. {@link Optional#empty()} if either
     * side is empty.
     */
    public static Optional<Double> ksStatistic(long[] baseline, long[] candidate) {
        if (baseline.length == 0 || candidate.length == 0) return Optional.empty();
        long[] a = baseline.clone();
        long[] b = candidate.clone();
        Arrays.sort(a);
        Arrays.sort(b);
        int i = 0, j = 0;
        double na = a.length;
        double nb = b.length;
        double maxD = 0.0;
        while (i < a.length && j < b.length) {
            if (a[i] <= b[j]) i++;
            else j++;
            double cdfA = i / na;
            double cdfB = j / nb;
            double d = Math.abs(cdfA - cdfB);
            if (d > maxD) maxD = d;
        }
        return Optional.of(maxD);
    }

    /**
     * Cohen's d effect size: standardised difference between two
     * means. Magnitude {@code < 0.2} small, {@code ~0.5} medium,
     * {@code > 0.8} large.
     */
    public static Optional<Double> cohensD(long[] baseline, long[] candidate) {
        if (baseline.length == 0 || candidate.length == 0) return Optional.empty();
        int na = baseline.length;
        int nb = candidate.length;
        double ma = Percentiles.mean(baseline);
        double mb = Percentiles.mean(candidate);
        double sa = Percentiles.stddev(baseline);
        double sb = Percentiles.stddev(candidate);
        double pooled = Math.sqrt(
                (((na - 1) * sa * sa) + ((nb - 1) * sb * sb)) / ((double) (na + nb - 2)));
        if (pooled <= 0.0) return Optional.of(0.0);
        return Optional.of((mb - ma) / pooled);
    }
}
