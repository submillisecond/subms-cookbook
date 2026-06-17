package com.submillisecond.recipes.ratelimit.features;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

final class DistributedLimiterTest {

    private static DistributedLimiter make(TestClock clk, long limit, long windowMs) {
        return new DistributedLimiter(new InMemoryBackend(), limit, windowMs * 1_000_000L, clk);
    }

    @Test
    void firstBurstGrantsUpToLimit() {
        TestClock clk = new TestClock();
        DistributedLimiter lim = make(clk, 5L, 1000L);
        for (int i = 0; i < 5; i++) assertTrue(lim.tryAcquire("user-1"));
        assertFalse(lim.tryAcquire("user-1"), "limit reached");
    }

    @Test
    void windowRollResetsCount() {
        TestClock clk = new TestClock();
        DistributedLimiter lim = make(clk, 3L, 100L);
        for (int i = 0; i < 3; i++) assertTrue(lim.tryAcquire("k"));
        assertFalse(lim.tryAcquire("k"));
        clk.advanceMs(150L);
        for (int i = 0; i < 3; i++) assertTrue(lim.tryAcquire("k"));
        assertFalse(lim.tryAcquire("k"));
    }

    @Test
    void keysAreIsolated() {
        TestClock clk = new TestClock();
        DistributedLimiter lim = make(clk, 2L, 1000L);
        for (int i = 0; i < 2; i++) assertTrue(lim.tryAcquire("a"));
        assertFalse(lim.tryAcquire("a"));
        for (int i = 0; i < 2; i++) assertTrue(lim.tryAcquire("b"));
    }

    @Test
    void backendSwapPreservesContract() {
        TestClock clk = new TestClock();
        InMemoryBackend shared = new InMemoryBackend();
        DistributedLimiter l1 = new DistributedLimiter(shared, 4L, 1_000_000_000L, clk);
        DistributedLimiter l2 = new DistributedLimiter(shared, 4L, 1_000_000_000L, clk);
        assertTrue(l1.tryAcquire("k"));
        assertTrue(l1.tryAcquire("k"));
        assertTrue(l2.tryAcquire("k"));
        assertTrue(l2.tryAcquire("k"));
        // 4 used across both via the shared backend.
        assertFalse(l1.tryAcquire("k"));
        assertFalse(l2.tryAcquire("k"));
    }

    @Test
    void readWithoutBumpIsObservation() {
        InMemoryBackend backend = new InMemoryBackend();
        assertEquals(0L, backend.read("k", 0L));
        backend.incr("k", 0L, 1_000_000L);
        backend.incr("k", 0L, 1_000_000L);
        assertEquals(2L, backend.read("k", 0L));
    }

    @Test
    void limitAndWindowAccessors() {
        TestClock clk = new TestClock();
        DistributedLimiter lim = make(clk, 42L, 250L);
        assertEquals(42L, lim.limit());
        assertEquals(250_000_000L, lim.windowNs());
    }
}
