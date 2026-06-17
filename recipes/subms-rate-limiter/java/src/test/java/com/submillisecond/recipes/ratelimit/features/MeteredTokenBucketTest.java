package com.submillisecond.recipes.ratelimit.features;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

final class MeteredTokenBucketTest {

    @Test
    void countsGrantedAndRejectedDistinctly() {
        TestClock clk = new TestClock();
        MeteredTokenBucket m = new MeteredTokenBucket(3L, 0.0, clk);
        assertTrue(m.tryAcquire(1L));
        assertTrue(m.tryAcquire(1L));
        assertTrue(m.tryAcquire(1L));
        assertFalse(m.tryAcquire(1L));
        assertFalse(m.tryAcquire(1L));
        MetricsSnapshot s = m.snapshot();
        assertEquals(3L, s.granted());
        assertEquals(2L, s.rejected());
    }

    @Test
    void snapshotReflectsCurrentTokens() {
        TestClock clk = new TestClock();
        MeteredTokenBucket m = new MeteredTokenBucket(5L, 0.0, clk);
        assertEquals(5L, m.snapshot().available());
        m.tryAcquire(2L);
        assertEquals(3L, m.snapshot().available());
    }

    @Test
    void refillEventsCountedWhenClockAdvances() {
        TestClock clk = new TestClock();
        MeteredTokenBucket m = new MeteredTokenBucket(5L, 100.0, clk); // 1 per 10 ms
        for (int i = 0; i < 5; i++) m.tryAcquire(1L);
        assertEquals(0L, m.snapshot().refills(), "no refill yet");
        clk.advanceMs(50L);
        assertTrue(m.tryAcquire(1L));
        assertTrue(m.snapshot().refills() >= 1L, "refill must be counted at least once");
    }

    @Test
    void burstAtFullDoesNotCountRefills() {
        TestClock clk = new TestClock();
        MeteredTokenBucket m = new MeteredTokenBucket(5L, 1000.0, clk);
        for (int i = 0; i < 5; i++) m.tryAcquire(1L);
        assertEquals(0L, m.snapshot().refills());
    }

    @Test
    void tryAcquireOneIncrementsGrantedByOne() {
        TestClock clk = new TestClock();
        MeteredTokenBucket m = new MeteredTokenBucket(2L, 0.0, clk);
        assertTrue(m.tryAcquireOne());
        assertEquals(1L, m.snapshot().granted());
    }

    @Test
    void capacityAndRatePassThrough() {
        TestClock clk = new TestClock();
        MeteredTokenBucket m = new MeteredTokenBucket(13L, 7.5, clk);
        assertEquals(13L, m.capacity());
        assertTrue(Math.abs(m.ratePerSec() - 7.5) < 0.01);
    }

    @Test
    void snapshotEqualityWorksForAssertions() {
        MetricsSnapshot s = new MetricsSnapshot(1L, 0L, 0L, 0L);
        assertEquals(s, new MetricsSnapshot(1L, 0L, 0L, 0L));
    }
}
