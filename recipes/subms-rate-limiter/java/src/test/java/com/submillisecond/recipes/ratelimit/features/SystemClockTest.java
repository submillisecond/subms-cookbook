package com.submillisecond.recipes.ratelimit.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The production clock every feature defaults to. It is four lines and it was
 * untested, which meant nothing checked the one property the limiters depend
 * on: that it starts at zero and never goes backwards.
 */
final class SystemClockTest {

    @Test
    void startsAtItsOwnOrigin() {
        SystemClock clock = new SystemClock();
        long first = clock.nowNs();
        assertTrue(first >= 0L, "the origin is the construction moment, so time starts at 0");
        assertTrue(first < 1_000_000_000L, "and a fresh clock is nowhere near a second in");
    }

    @Test
    void isMonotonicNonDecreasing() {
        SystemClock clock = new SystemClock();
        long previous = clock.nowNs();
        for (int i = 0; i < 1000; i++) {
            long now = clock.nowNs();
            assertTrue(now >= previous, "the clock went backwards: " + previous + " then " + now);
            previous = now;
        }
    }

    @Test
    void twoClocksKeepIndependentOrigins() {
        SystemClock first = new SystemClock();
        // Burn a measurable amount of time so the second clock's origin is
        // strictly later than the first's.
        long spin = 0L;
        while (first.nowNs() < 1_000_000L) {
            spin++;
        }
        SystemClock second = new SystemClock();
        assertTrue(spin > 0L);
        assertTrue(second.nowNs() < first.nowNs(),
                "a later clock reports less elapsed time than an earlier one");
    }

    @Test
    void drivesATokenBucketRefill() {
        // The default constructor path: no clock argument means SystemClock.
        TokenBucket bucket = new TokenBucket(2, 1_000_000.0);
        assertEquals(2L, bucket.capacity());
        assertTrue(bucket.tryAcquire(2));
        // At a million tokens a second the bucket refills within the time this
        // loop takes, which is the whole point of wiring a real clock in.
        boolean refilled = false;
        for (int i = 0; i < 1_000_000 && !refilled; i++) {
            refilled = bucket.tryAcquireOne();
        }
        assertTrue(refilled, "a wall-clock-driven bucket must refill on its own");
    }

    @Test
    void isTheDefaultForEveryFeatureThatTakesAClock() {
        // These single-argument constructors exist so a caller who does not
        // care about determinism never has to name a clock. Each was
        // previously unexercised.
        assertEquals(4L, new TokenBucket(4, 10.0).capacity());
        assertEquals(4L, new MeteredTokenBucket(4, 10.0).capacity());
        assertTrue(new MeteredTokenBucket(4, 10.0).tryAcquireOne());

        HierarchicalLimiter desk = new HierarchicalLimiter(5, 1.0, 2, 10, 1.0);
        assertEquals(2, desk.numChildren());
        assertTrue(desk.tryAcquire(0, 1), "the wall-clock hierarchy admits from a full parent");

        DistributedLimiter router = new DistributedLimiter(new InMemoryBackend(), 2, 1_000_000_000L);
        assertEquals(2L, router.limit());
        assertTrue(router.tryAcquire("acct-1"));
        assertTrue(router.tryAcquire("acct-1"));
        assertFalse(router.tryAcquire("acct-1"), "the third request exceeds a limit of 2");
    }
}
