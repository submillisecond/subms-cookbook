package com.submillisecond.recipes.stats;

import java.util.Optional;

/**
 * Sample app: a tour of {@code subms-stats} over a batch of order-ack latencies
 * (nanoseconds) captured off a trading gateway. Every analysis reads the
 * already-recorded sample array; the recipe never touches the hot path that
 * produced it. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.stats.SampleApp}
 *
 * <ul>
 *   <li>base      - p50/p99/p999, mean, stddev over the ack-latency batch
 *   <li>histogram - a log2-spaced CDF for exporting the whole distribution
 *   <li>jitter    - was the measurement rig stable across the run?
 *   <li>tail      - conditional tail expectation, Hill index, fatness ratio
 *   <li>robust    - IQR, MAD, CoV, skewness, kurtosis (outlier-resistant spread)
 *   <li>compare   - KS statistic + Cohen's d between a baseline and a candidate
 *   <li>bootstrap - a confidence interval around the p99 point estimate
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) {
        long[] acks = ackLatenciesNs();
        baseSummary(acks);
        histogramCdf(acks);
        jitterStability(acks);
        tailShape(acks);
        robustSpread(acks);
        compareBaselineCandidate();
        bootstrapP99Interval(acks);
    }

    /** Base API: headline percentiles plus first-moment stats over the batch. */
    static void baseSummary(long[] acks) {
        System.out.println("== base: order-ack latency summary (" + acks.length + " samples) ==");
        SubMsSamples s = SubMsSamples.of(acks);
        System.out.println("  p50 " + s.p50() + " ns");
        System.out.println("  p99 " + s.p99() + " ns");
        System.out.println("  p999 " + s.p999() + " ns");
        System.out.println("  mean " + s.mean() + " ns  stddev " + s.stddev() + " ns  max " + s.max() + " ns");
        if (s.p99() < s.p50()) throw new AssertionError("the tail never sits below the median");
        if (s.max() < s.p999()) throw new AssertionError("max bounds every percentile");
    }

    /** histogram: a log2-spaced CDF; downstream tooling rebuilds any quantile from the buckets. */
    static void histogramCdf(long[] acks) {
        System.out.println("\n== histogram: log2 CDF buckets ==");
        long[] buckets = SubMsSamples.of(acks).cdfBuckets();
        long total = 0;
        int modal = 0;
        for (int i = 0; i < buckets.length; i++) {
            total += buckets[i];
            if (buckets[i] > buckets[modal]) modal = i;
        }
        System.out.println("  " + buckets.length + " buckets, " + total + " samples total");
        System.out.println("  modal bucket " + modal + " covers [2^" + modal + ", 2^" + (modal + 1) + ") ns");
        if (total != acks.length) throw new AssertionError("every sample lands in exactly one bucket");
    }

    /** jitter: cross-window CoV. High jitter is a moving rig, not a slower gateway. */
    static void jitterStability(long[] acks) {
        System.out.println("\n== jitter: measurement-rig stability ==");
        double score = SubMsSamples.of(acks).jitterScore();
        System.out.printf("  jitter score %.4f (0.0 clean, 1.0 hostile)%n", score);
        if (score < 0.0 || score > 1.0) throw new AssertionError("score stays in the unit interval");
    }

    /** tail: what a lone p99 hides - CTE, fatness ratio, Hill index. */
    static void tailShape(long[] acks) {
        System.out.println("\n== tail: heavy-tail diagnostics ==");
        SubMsSamples s = SubMsSamples.of(acks);
        long cte99 = s.conditionalTailExpectation(0.99);
        double fatness = s.tailFatnessRatio();
        System.out.println("  CTE(0.99) " + cte99 + " ns  (mean of the worst 1%)");
        System.out.printf("  fatness p99/p50 %.2f%n", fatness);
        Optional<Double> hill = s.hillTailIndex(64);
        hill.ifPresent(h -> System.out.printf("  Hill index (top 64) %.3f%n", h));
        if (cte99 < s.p99()) throw new AssertionError("the worst-1% mean sits at or above p99");
        if (fatness <= 1.0) throw new AssertionError("the injected spikes make the tail fatter than uniform");
    }

    /** robust: outlier-resistant spread - IQR, MAD, CoV, skewness, kurtosis. */
    static void robustSpread(long[] acks) {
        System.out.println("\n== robust: outlier-resistant spread ==");
        SubMsSamples s = SubMsSamples.of(acks);
        System.out.println("  IQR " + s.iqr() + " ns  MAD " + s.medianAbsoluteDeviation() + " ns");
        System.out.printf("  CoV %.4f%n", s.coefficientOfVariation());
        System.out.printf("  skewness %.3f  excess kurtosis %.3f%n", s.skewness(), s.kurtosis());
        if (s.iqr() == 0) throw new AssertionError("the body of the distribution has real spread");
        if (s.skewness() <= 0.0) throw new AssertionError("latency skews right");
    }

    /** compare: did a deploy shift the distribution? KS gap + Cohen's d. */
    static void compareBaselineCandidate() {
        System.out.println("\n== compare: baseline vs candidate deploy ==");
        long[] baseline = ackLatenciesNs();
        long[] candidate = new long[baseline.length];
        for (int i = 0; i < baseline.length; i++) candidate[i] = baseline[i] + 120;
        double ks = Compare.ksStatistic(baseline, candidate).orElseThrow();
        double d = Compare.cohensD(baseline, candidate).orElseThrow();
        System.out.printf("  KS statistic %.3f%n", ks);
        System.out.printf("  Cohen's d %.3f (candidate is +120 ns slower)%n", d);
        if (d <= 0.0) throw new AssertionError("the uniformly-slower candidate lifts the mean");
    }

    /** bootstrap: how wide is the p99 estimate? Deterministic under a fixed seed. */
    static void bootstrapP99Interval(long[] acks) {
        System.out.println("\n== bootstrap: confidence interval around p99 ==");
        SubMsSamples s = SubMsSamples.of(acks);
        Bootstrap.CI ci = s.bootstrapPercentileCi(0.99, 500, 0.95, 42);
        System.out.println("  p99 point " + s.p99() + " ns");
        System.out.println("  95% CI [" + ci.lo() + " ns, " + ci.hi() + " ns]");
        if (ci.lo() > ci.hi()) throw new AssertionError("the interval is ordered");
        if (ci.lo() > s.p99() || s.p99() > ci.hi()) throw new AssertionError("the point estimate sits inside its CI");
    }

    /**
     * A deterministic right-skewed ack-latency batch: a tight body around 800 ns
     * with periodic spikes near 5 us, so the tail and robust sections have real
     * shape to report. Byte-for-byte the same stream as the Rust sample.
     */
    static long[] ackLatenciesNs() {
        long[] out = new long[2_048];
        long state = 0x9E3779B97F4A7C15L;
        for (int i = 0; i < out.length; i++) {
            state = state * 6364136223846793005L + 1442695040888963407L;
            long body = 760 + Long.remainderUnsigned(state >>> 40, 120);
            out[i] = (i % 97 == 0)
                ? 4_800 + Long.remainderUnsigned(state >>> 32, 600)
                : body;
        }
        return out;
    }

    private SampleApp() {}
}
