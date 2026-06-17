package com.submillisecond.recipes.ratelimit.features;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

final class TokenBucketTest {

    @Test
    void burstAtFullBucketDrainsCapacity() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(10L, 100.0, clk);
        for (int i = 0; i < 10; i++) {
            assertTrue(tb.tryAcquire(1L), "draw " + i + " should succeed");
        }
        assertFalse(tb.tryAcquire(1L), "11th draw should fail with no refill");
    }

    @Test
    void refillOverTimeGapReplenishesBucket() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(10L, 100.0, clk); // 1 per 10 ms
        for (int i = 0; i < 10; i++) tb.tryAcquire(1L);
        assertFalse(tb.tryAcquire(1L));
        clk.advanceMs(50L);
        for (int i = 0; i < 5; i++) assertTrue(tb.tryAcquire(1L));
        assertFalse(tb.tryAcquire(1L));
    }

    @Test
    void refillCapsAtCapacity() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(5L, 10.0, clk);
        for (int i = 0; i < 5; i++) tb.tryAcquire(1L);
        clk.advanceMs(10_000L);
        assertEquals(5L, tb.available(), "refill must cap at capacity");
    }

    @Test
    void batchAcquireSucceedsOrFailsAtomically() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(10L, 0.0, clk);
        assertTrue(tb.tryAcquire(7L));
        assertFalse(tb.tryAcquire(5L), "would exceed remaining 3");
        assertTrue(tb.tryAcquire(3L));
        assertFalse(tb.tryAcquire(1L));
    }

    @Test
    void zeroAcquireIsAlwaysTrueAndDoesNotSpend() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(3L, 0.0, clk);
        assertTrue(tb.tryAcquire(0L));
        assertEquals(3L, tb.available());
    }

    @Test
    void tryAcquireOneIsShorthandForOne() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(2L, 0.0, clk);
        assertTrue(tb.tryAcquireOne());
        assertTrue(tb.tryAcquireOne());
        assertFalse(tb.tryAcquireOne());
    }

    @Test
    void fractionalRefillAccumulatesWithoutLoss() {
        TestClock clk = new TestClock();
        TokenBucket tb = new TokenBucket(5L, 1.0, clk); // 1/sec
        for (int i = 0; i < 5; i++) tb.tryAcquire(1L);
        for (int i = 0; i < 10; i++) clk.advanceMs(100L);
        assertEquals(1L, tb.available(), "fractional refills must accumulate");
    }

    @Test
    void accessorsReflectConstruction() {
        TokenBucket tb = new TokenBucket(8L, 250.0, new TestClock());
        assertEquals(8L, tb.capacity());
        assertTrue(Math.abs(tb.ratePerSec() - 250.0) < 0.01);
    }
}
