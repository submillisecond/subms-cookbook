package com.submillisecond.primers.stats;

import com.submillisecond.stats.Bootstrap;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Exercises every wrapper {@link Analyser} touches against synthetic
 * baseline + candidate streams. A regression engineered into
 * {@link Workload.TailShape#HEAVY} must surface in:
 *
 * <ul>
 *   <li>headline percentiles (candidate p99 &gt; baseline p99),</li>
 *   <li>tail measures (candidate fatness &gt; baseline fatness),</li>
 *   <li>compare block (KS &gt; 0, Cohen's d &gt; 0).</li>
 * </ul>
 */
final class AnalyserTest {

    private static final int SAMPLE_COUNT = 20_000;
    private static final long BASE_NS = 800_000L;

    private long[] baseline() {
        return Workload.generate(SAMPLE_COUNT, BASE_NS, Workload.TailShape.CLEAN, 1L);
    }

    private long[] candidate() {
        return Workload.generate(SAMPLE_COUNT, BASE_NS, Workload.TailShape.HEAVY, 2L);
    }

    @Test
    @DisplayName("percentiles are monotone non-decreasing across p50/p90/p99/p99.9/max")
    void percentilesAreMonotone() {
        StatsReport r = Analyser.analyse("base", baseline());
        assertTrue(r.p50() <= r.p90(),  "p50 must be <= p90");
        assertTrue(r.p90() <= r.p99(),  "p90 must be <= p99");
        assertTrue(r.p99() <= r.p999(), "p99 must be <= p99.9");
        assertTrue(r.p999() <= r.max(), "p99.9 must be <= max");
        assertEquals(SAMPLE_COUNT, r.count(), "count must reflect the sample array length");
    }

    @Test
    @DisplayName("mean/stddev/iqr/mad are non-negative; cv is finite")
    void momentsAndSpread() {
        StatsReport r = Analyser.analyse("base", baseline());
        assertTrue(r.mean()   > 0, "mean must be positive on a positive sample stream");
        assertTrue(r.stddev() > 0, "stddev must be positive on a non-constant stream");
        assertTrue(r.iqr()    > 0, "iqr must be positive on a non-constant stream");
        assertTrue(r.mad()    > 0, "mad must be positive on a non-constant stream");
        assertTrue(Double.isFinite(r.cv())       && r.cv()       >= 0.0, "cv finite + non-negative");
        assertTrue(Double.isFinite(r.skewness()),  "skewness must be finite");
        assertTrue(Double.isFinite(r.kurtosis()),  "kurtosis must be finite");
    }

    @Test
    @DisplayName("tail measures populate; hill is Optional and present at k=50 with 20k samples")
    void tailMeasures() {
        StatsReport r = Analyser.analyse("base", baseline());
        assertTrue(r.cte99() >= r.p99(),
                "conditional tail expectation must be >= p99 (it's the mean above the cutoff)");
        assertTrue(r.tailFatness() >= 1.0,
                "p99/p50 ratio must be >= 1.0 for a non-degenerate latency stream");
        Optional<Double> hill = r.hillIndex();
        assertTrue(hill.isPresent(), "Hill estimator should populate with 20k samples and k=50");
        assertTrue(Double.isFinite(hill.get()) && hill.get() > 0.0,
                "Hill index must be a positive finite number on a positive-valued stream");
    }

    @Test
    @DisplayName("jitter score is in [0.0, 1.0]")
    void jitterRange() {
        StatsReport r = Analyser.analyse("base", baseline());
        double j = r.jitterScore();
        assertTrue(j >= 0.0 && j <= 1.0,
                "jitterScore must be clamped to [0.0, 1.0]; got " + j);
    }

    @Test
    @DisplayName("bootstrap CI brackets the point p99 (or sits very close)")
    void bootstrapCiBracketsP99() {
        StatsReport r = Analyser.analyse("base", baseline());
        Bootstrap.CI ci = r.p99Ci();
        assertNotNull(ci);
        assertTrue(ci.lo() <= ci.hi(), "CI lo must be <= hi");
        // The point p99 of the original sample is one possible resample percentile,
        // so the CI should not be wildly off from it. Allow generous margin for
        // resample variability at iters=500.
        long p99 = r.p99();
        long width = Math.max(1L, ci.hi() - ci.lo());
        long pad = Math.max(p99 / 4, width);
        assertTrue(p99 >= ci.lo() - pad && p99 <= ci.hi() + pad,
                "point p99 must be near the bootstrap CI; p99=" + p99 + " ci=[" + ci.lo() + "," + ci.hi() + "]");
    }

    @Test
    @DisplayName("candidate (HEAVY) has a heavier tail than baseline (CLEAN)")
    void candidateIsHeavier() {
        StatsReport base = Analyser.analyse("base", baseline());
        StatsReport cand = Analyser.analyse("cand", candidate());
        assertTrue(cand.tailFatness() > base.tailFatness(),
                "candidate p99/p50 should exceed baseline; base=" + base.tailFatness()
                        + " cand=" + cand.tailFatness());
        assertTrue(cand.cte99() > base.cte99(),
                "candidate cte99 should exceed baseline; base=" + base.cte99()
                        + " cand=" + cand.cte99());
        assertTrue(cand.p99() > base.p99(),
                "candidate p99 should exceed baseline; base=" + base.p99() + " cand=" + cand.p99());
    }

    @Test
    @DisplayName("Compare.ksStatistic + cohensD detect the engineered regression")
    void compareDetectsRegression() {
        long[] b = baseline();
        long[] c = candidate();
        Optional<Double> ks = Analyser.ks(b, c);
        Optional<Double> d  = Analyser.cohensD(b, c);
        assertTrue(ks.isPresent(),    "KS statistic must populate for two non-empty arrays");
        assertTrue(d.isPresent(),     "Cohen's d must populate for two non-empty arrays");
        assertTrue(ks.get() > 0.0,    "KS must be > 0 between distinct distributions; got " + ks.get());
        assertTrue(d.get() > 0.0,
                "Cohen's d must be > 0 when candidate mean exceeds baseline; got " + d.get());
    }

    @Test
    @DisplayName("Analyser is a pure function: same input -> same StatsReport")
    void analyserIsPure() {
        long[] s = baseline();
        StatsReport r1 = Analyser.analyse("x", s);
        StatsReport r2 = Analyser.analyse("x", s);
        assertEquals(r1.p99(),     r2.p99());
        assertEquals(r1.mean(),    r2.mean());
        assertEquals(r1.cte99(),   r2.cte99());
        assertEquals(r1.iqr(),     r2.iqr());
        assertEquals(r1.p99Ci().lo(), r2.p99Ci().lo());
        assertEquals(r1.p99Ci().hi(), r2.p99Ci().hi());
    }
}
