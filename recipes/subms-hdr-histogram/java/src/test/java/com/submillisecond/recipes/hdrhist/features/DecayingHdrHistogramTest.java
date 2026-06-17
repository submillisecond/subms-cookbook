package com.submillisecond.recipes.hdrhist.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class DecayingHdrHistogramTest {

    @Test
    void emptyDecayIsZero() {
        ManualClock clk = new ManualClock();
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, 1_000_000_000L, clk);
        assertEquals(0.0, h.count(), 1e-9);
        assertEquals(0, h.max());
        assertEquals(0, h.valueAtPercentile(0.99));
    }

    @Test
    void noTimePassingMeansNoDecay() {
        ManualClock clk = new ManualClock();
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, 1_000_000_000L, clk);
        for (long v = 1; v <= 100; v++) h.record(v);
        double c = h.count();
        assertEquals(100.0, c, 1e-6, "no time passed");
    }

    @Test
    void oneHalflifeHalvesCount() {
        ManualClock clk = new ManualClock();
        long halflife = 1_000_000_000L;
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clk);
        for (int i = 0; i < 1000; i++) h.record(50);
        clk.advanceNs(halflife);
        double c = h.count();
        assertEquals(500.0, c, 1.0, "halflife should halve, got " + c);
    }

    @Test
    void twoHalflivesQuarterCount() {
        ManualClock clk = new ManualClock();
        long halflife = 500_000_000L;
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clk);
        for (int i = 0; i < 1000; i++) h.record(100);
        clk.advanceNs(halflife * 2);
        double c = h.count();
        assertEquals(250.0, c, 1.0, "two halflives -> 1/4, got " + c);
    }

    @Test
    void recentRecordsOutweighOld() {
        ManualClock clk = new ManualClock();
        long halflife = 1_000_000_000L;
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clk);
        for (int i = 0; i < 100; i++) h.record(10);
        clk.advanceNs(halflife * 4);
        for (int i = 0; i < 100; i++) h.record(1000);
        long p50 = h.valueAtPercentile(0.5);
        assertTrue(p50 >= 500, "recent bucket dominates, p50=" + p50);
    }

    @Test
    void longIdleCollapsesToZero() {
        ManualClock clk = new ManualClock();
        long halflife = 1_000_000_000L;
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clk);
        for (int i = 0; i < 1000; i++) h.record(50);
        clk.advanceNs(halflife * 30);
        double c = h.count();
        assertTrue(c < 1e-6, "30 half-lives -> ~0, got " + c);
    }

    @Test
    void writeDuringDecayCompetesFairly() {
        ManualClock clk = new ManualClock();
        long halflife = 1_000_000_000L;
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clk);
        h.record(100);
        clk.advanceNs(halflife);
        h.record(200);
        double total = h.count();
        assertEquals(1.5, total, 0.05, "weighted total ~ 1.5, got " + total);
    }
}
