package com.submillisecond.recipes.tsanomaly;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Optional;

import org.junit.jupiter.api.Test;

class TsAnomalyDetectorTest {

    @Test
    void warmupReturnsNone() {
        TsAnomalyDetector d = new TsAnomalyDetector(1_000, 3.0);
        assertTrue(d.push(0, 5.0).isEmpty()); // n=0 prior
        assertTrue(d.push(1, 5.0).isEmpty()); // n=1 prior
        assertTrue(d.push(2, 5.0).isEmpty()); // 2 prior, still stable
    }

    @Test
    void stableSeriesNoAnomalies() {
        TsAnomalyDetector d = new TsAnomalyDetector(10_000, 3.0);
        int flags = 0;
        for (int i = 0; i < 200; i++) {
            double v = 10.0 + (i % 2 == 0 ? -1.0 : 1.0);
            if (d.push(i, v).isPresent()) flags++;
        }
        assertEquals(0, flags);
    }

    @Test
    void spikeFlagged() {
        TsAnomalyDetector d = new TsAnomalyDetector(10_000, 3.0);
        for (int i = 0; i < 50; i++) {
            d.push(i, 10.0 + (i % 2) * 0.1);
        }
        Optional<TsAnomaly> hit = d.push(50, 100.0);
        assertTrue(hit.isPresent());
        TsAnomaly a = hit.get();
        assertEquals(50, a.ts());
        assertEquals(100.0, a.value());
        assertTrue(a.zscore() > 3.0, "z=" + a.zscore());
    }

    @Test
    void jumpOffFlatBaselineFlags() {
        TsAnomalyDetector d = new TsAnomalyDetector(10_000, 3.0);
        for (int i = 0; i < 30; i++) {
            d.push(i, 7.0);
        }
        Optional<TsAnomaly> hit = d.push(30, 8.0);
        assertTrue(hit.isPresent());
        assertTrue(Double.isFinite(hit.get().zscore()));
    }

    @Test
    void negativeSpikeHasNegativeZ() {
        TsAnomalyDetector d = new TsAnomalyDetector(10_000, 3.0);
        for (int i = 0; i < 50; i++) {
            d.push(i, 100.0 + (i % 2) * 0.1);
        }
        TsAnomaly a = d.push(50, 1.0).orElseThrow();
        assertTrue(a.zscore() < -3.0, "z=" + a.zscore());
    }

    @Test
    void sigmaThresholdRespected() {
        TsAnomalyDetector d = new TsAnomalyDetector(1_000_000, 2.0);
        for (int i = 0; i < 100; i++) {
            d.push(i, i);
        }
        // ~1 std above mean should NOT flag at 2 sigma
        assertTrue(d.push(100, 78.0).isEmpty());
        // ~3 std above should flag
        assertTrue(d.push(101, 140.0).isPresent());
    }

    @Test
    void windowExpiryShiftsBaseline() {
        TsAnomalyDetector d = new TsAnomalyDetector(100, 3.0);
        for (int i = 0; i < 50; i++) {
            d.push(i, 0.0 + (i % 2) * 0.01);
        }
        int laterFlags = 0;
        for (int i = 200; i < 300; i++) {
            if (d.push(i, 50.0 + (i % 2) * 0.01).isPresent()) laterFlags++;
        }
        assertEquals(0, laterFlags, "baseline should have adapted to the new regime");
    }

    @Test
    void windowCountTracks() {
        TsAnomalyDetector d = new TsAnomalyDetector(100, 3.0);
        d.push(0, 1.0);
        d.push(50, 2.0);
        d.push(90, 3.0);
        assertEquals(3, d.windowCount());
        d.push(201, 4.0); // cutoff 101 -> 0,50,90 all expire
        assertEquals(1, d.windowCount());
    }

    @Test
    void flatBaselineSameValueNoFlag() {
        TsAnomalyDetector d = new TsAnomalyDetector(10_000, 3.0);
        for (int i = 0; i < 30; i++) {
            d.push(i, 7.0);
        }
        // same value as the flat baseline -> z = 0, no flag
        assertTrue(d.push(30, 7.0).isEmpty());
    }

    @Test
    void crossCheckZscoreAgainstManual() {
        TsAnomalyDetector d = new TsAnomalyDetector(1_000_000, 0.0); // sigma 0 -> always flags, read z
        d.push(0, 2.0);
        d.push(1, 4.0);
        // prior window {2,4}: mean 3, var = (4+16)/2 - 9 = 1, std 1
        TsAnomaly a = d.push(2, 6.0).orElseThrow();
        assertTrue(Math.abs(a.zscore() - 3.0) < 1e-9, "z=" + a.zscore()); // (6-3)/1
    }

    @Test
    void windowNsFlooredAtOne() {
        // windowNs <= 0 is floored to 1, so each push expires all prior points
        // (front.ts <= ts - 1) and the window never holds 2 priors -> no flags
        TsAnomalyDetector d = new TsAnomalyDetector(0, 3.0);
        d.push(0, 1.0);
        d.push(1, 100.0);
        assertTrue(d.push(2, 1000.0).isEmpty());
        assertFalse(d.windowCount() >= 2);
    }
}
