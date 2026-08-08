package com.submillisecond.recipes.stats;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        long[] raw = {100, 200, 150, 300, 250, 175, 125, 400};
        SubMsSamples s = SubMsSamples.of(raw);
        assertTrue(s.p99() >= s.p50());   // the tail never sits below the median
        assertTrue(s.max() >= s.p99());   // max bounds every percentile
        // quickstart:end
    }

    @Test
    void baseSummaryIsConsistent() {
        long[] acks = SampleApp.ackLatenciesNs();
        SubMsSamples s = SubMsSamples.of(acks);
        assertEquals(2_048, s.count());
        assertTrue(s.p50() <= s.p99(), "median under the tail");
        assertTrue(s.p99() <= s.p999(), "p99 under p999");
        assertTrue(s.p999() <= s.max(), "max bounds every percentile");
        assertTrue(s.mean() > 0, "a non-empty batch has a positive mean");
    }

    @Test
    void histogramCoversEverySample() {
        long[] acks = SampleApp.ackLatenciesNs();
        long[] buckets = SubMsSamples.of(acks).cdfBuckets();
        assertEquals(64, buckets.length);
        long total = 0;
        for (long b : buckets) total += b;
        assertEquals(acks.length, total, "no sample is lost or double counted");
    }

    @Test
    void jitterScoreInUnitInterval() {
        long[] acks = SampleApp.ackLatenciesNs();
        double score = SubMsSamples.of(acks).jitterScore();
        assertTrue(score >= 0.0 && score <= 1.0, "score clamps to [0, 1]");
    }

    @Test
    void tailReflectsInjectedSpikes() {
        long[] acks = SampleApp.ackLatenciesNs();
        SubMsSamples s = SubMsSamples.of(acks);
        assertTrue(s.conditionalTailExpectation(0.99) >= s.p99(), "worst-1% mean >= p99");
        assertTrue(s.tailFatnessRatio() > 1.0, "spikes make the tail fatter than uniform");
    }

    @Test
    void robustSpreadShowsRightSkew() {
        long[] acks = SampleApp.ackLatenciesNs();
        SubMsSamples s = SubMsSamples.of(acks);
        assertTrue(s.iqr() > 0, "the body has spread");
        assertTrue(s.skewness() > 0.0, "latency skews right");
    }

    @Test
    void compareFlagsASlowerCandidate() {
        long[] baseline = SampleApp.ackLatenciesNs();
        long[] candidate = new long[baseline.length];
        for (int i = 0; i < baseline.length; i++) candidate[i] = baseline[i] + 120;
        assertTrue(Compare.ksStatistic(baseline, candidate).orElseThrow() > 0.0, "the CDFs differ");
        assertTrue(Compare.cohensD(baseline, candidate).orElseThrow() > 0.0, "the candidate is slower");
    }

    @Test
    void bootstrapCiBracketsThePointEstimate() {
        long[] acks = SampleApp.ackLatenciesNs();
        SubMsSamples s = SubMsSamples.of(acks);
        Bootstrap.CI ci = s.bootstrapPercentileCi(0.99, 500, 0.95, 42);
        assertTrue(ci.lo() <= ci.hi(), "ordered interval");
        assertTrue(ci.lo() <= s.p99() && s.p99() <= ci.hi(), "point estimate inside its CI");
    }
}
