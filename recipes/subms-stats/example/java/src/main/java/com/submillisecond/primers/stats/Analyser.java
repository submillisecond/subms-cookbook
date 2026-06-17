package com.submillisecond.primers.stats;

import com.submillisecond.stats.Bootstrap;
import com.submillisecond.stats.Compare;
import com.submillisecond.stats.SubMsSamples;
import com.submillisecond.stats.Tail;

import java.util.Optional;

/**
 * End-to-end walk through the subms-stats public surface. Hands a
 * {@code long[]} of nanosecond samples to {@link SubMsSamples} for the
 * headline percentiles, then layers in the tail / robust / jitter /
 * bootstrap analyses around it. Pure function: same input,
 * byte-identical output (the bootstrap CI uses a fixed seed).
 */
public final class Analyser {

    /** Hill estimator depth - the top {@code HILL_K} order statistics. */
    private static final int HILL_K = 50;

    /** Bootstrap iteration count - tens of ms at 50k samples; off the hot path. */
    private static final int BOOT_ITERS = 500;

    /** Bootstrap confidence level. 0.95 is the usual reach. */
    private static final double BOOT_CONFIDENCE = 0.95;

    /** Bootstrap RNG seed. Fixed so the report is reproducible. */
    private static final long BOOT_SEED = 0L;

    private Analyser() {}

    /**
     * Run every subms-stats wrapper against {@code samples} and return
     * a typed report.
     */
    public static StatsReport analyse(String label, long[] samples) {
        SubMsSamples s = SubMsSamples.of(samples);

        long p50  = s.p50();
        long p90  = s.p90();
        long p99  = s.p99();
        long p999 = s.p999();
        long max  = s.max();
        long mean = s.mean();
        long std  = s.stddev();

        long cte99 = Tail.conditionalTailExpectation(samples, 0.99);
        double fatness = Tail.tailFatnessRatio(samples);
        Optional<Double> hill = Tail.hillTailIndex(samples, HILL_K);

        long iqr = s.iqr();
        long mad = s.medianAbsoluteDeviation();
        double cv = s.coefficientOfVariation();
        double skew = s.skewness();
        double kurt = s.kurtosis();

        double jitter = s.jitterScore();

        Bootstrap.CI p99Ci = s.bootstrapPercentileCi(0.99, BOOT_ITERS, BOOT_CONFIDENCE, BOOT_SEED);

        return new StatsReport(
                label, samples.length, p50, p90, p99, p999, max, mean, std,
                cte99, fatness, hill,
                iqr, mad, cv, skew, kurt,
                jitter, p99Ci);
    }

    /**
     * KS statistic between two runs. {@link Optional#empty()} if either
     * side is empty.
     */
    public static Optional<Double> ks(long[] baseline, long[] candidate) {
        return Compare.ksStatistic(baseline, candidate);
    }

    /**
     * Cohen's d effect size between two runs. Positive = candidate
     * shifted toward higher latencies.
     */
    public static Optional<Double> cohensD(long[] baseline, long[] candidate) {
        return Compare.cohensD(baseline, candidate);
    }
}
