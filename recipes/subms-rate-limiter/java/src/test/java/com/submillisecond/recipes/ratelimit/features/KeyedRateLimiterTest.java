package com.submillisecond.recipes.ratelimit.features;

import com.submillisecond.recipes.ratelimit.RateLimiter;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class KeyedRateLimiterTest {

    private static final long MS = 1_000_000L;

    /** 1000/sec => period 1ms; burst 3 => window 3ms. */
    private static KeyedRateLimiter limiter() {
        return new KeyedRateLimiter(1000.0, 3);
    }

    @Test
    void keysAreIndependent() {
        KeyedRateLimiter k = limiter();
        for (int i = 0; i < 3; i++) {
            assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(0L, "acct-a", 1L));
        }
        assertInstanceOf(RateLimiter.Acquire.Retry.class, k.tryAcquireAt(0L, "acct-a", 1L));
        // A saturated key must not throttle its neighbour.
        assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(0L, "acct-b", 1L));
    }

    @Test
    void perKeyRefillFollowsTheClock() {
        KeyedRateLimiter k = limiter();
        for (int i = 0; i < 3; i++) k.tryAcquireAt(0L, "sym-ESU5", 1L);
        assertInstanceOf(RateLimiter.Acquire.Retry.class, k.tryAcquireAt(0L, "sym-ESU5", 1L));
        assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(1 * MS, "sym-ESU5", 1L));
        assertInstanceOf(RateLimiter.Acquire.Retry.class,
                k.tryAcquireAt(1_500_000L, "sym-ESU5", 1L));
    }

    @Test
    void retryAfterMatchesTheBaseLimiter() {
        KeyedRateLimiter k = limiter();
        for (int i = 0; i < 3; i++) k.tryAcquireAt(0L, "acct-a", 1L);
        RateLimiter.Acquire.Retry r = assertInstanceOf(
                RateLimiter.Acquire.Retry.class, k.tryAcquireAt(0L, "acct-a", 1L));
        assertEquals(Duration.ofNanos(1_000_000L), r.retryAfter());
    }

    @Test
    void weightedDrawCostsNPeriods() {
        KeyedRateLimiter k = limiter();
        assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(0L, "acct-a", 3L));
        assertInstanceOf(RateLimiter.Acquire.Retry.class, k.tryAcquireAt(0L, "acct-a", 1L));
    }

    @Test
    void weightAboveBurstIsUnattainable() {
        KeyedRateLimiter k = limiter();
        RateLimiter.Acquire.Unattainable u = assertInstanceOf(
                RateLimiter.Acquire.Unattainable.class, k.tryAcquireAt(0L, "acct-a", 4L));
        assertEquals(3L, u.burstCapacity());
        assertTrue(k.isEmpty(), "an unattainable request must not have left state behind");
    }

    @Test
    void zeroWeightIsAFreeProbeAndNegativeIsAnError() {
        KeyedRateLimiter k = limiter();
        assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(0L, "acct-a", 0L));
        assertTrue(k.isEmpty(), "a zero-weight probe tracks no state");
        assertThrows(IllegalArgumentException.class, () -> k.tryAcquireAt(0L, "acct-a", -1L));
    }

    @Test
    void trackedKeyCountFollowsGrantedKeys() {
        KeyedRateLimiter k = limiter();
        for (int i = 0; i < 3; i++) k.tryAcquireAt(0L, "hot", 1L);
        k.tryAcquireAt(0L, "warm", 1L);
        assertEquals(2, k.size());
        // An oversized request is answered before the map is touched, so a
        // caller cannot grow the key set with requests that can never be
        // granted.
        assertInstanceOf(RateLimiter.Acquire.Unattainable.class,
                k.tryAcquireAt(0L, "never-granted", 9L));
        assertEquals(2, k.size());
    }

    @Test
    void timeUntilReadyDoesNotSpend() {
        KeyedRateLimiter k = limiter();
        assertEquals(Duration.ZERO, k.timeUntilReadyAt(0L, "acct-a", 3L).orElseThrow());
        assertEquals(Duration.ZERO, k.timeUntilReadyAt(0L, "acct-a", 3L).orElseThrow());
        assertTrue(k.isEmpty(), "a peek materialises nothing");
        assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(0L, "acct-a", 3L));
        assertEquals(Duration.ofNanos(1_000_000L),
                k.timeUntilReadyAt(0L, "acct-a", 1L).orElseThrow());
        assertTrue(k.timeUntilReadyAt(0L, "acct-a", 9L).isEmpty());
        assertEquals(Duration.ZERO, k.timeUntilReadyAt(0L, "acct-a", 0L).orElseThrow());
    }

    @Test
    void retainActiveEvictsIdleKeysOnly() {
        KeyedRateLimiter k = limiter();
        k.tryAcquireAt(0L, "idle", 1L);          // tat = 1ms
        k.tryAcquireAt(5 * MS, "busy", 3L);      // tat = 8ms
        assertEquals(2, k.size());

        assertEquals(1, k.retainActiveAt(5 * MS), "the idle key goes");
        assertEquals(1, k.size());

        // Eviction is lossless: the evicted key comes back at full burst, which
        // is what it had anyway after 5ms of idling.
        for (int i = 0; i < 3; i++) {
            assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(5 * MS, "idle", 1L));
        }
    }

    @Test
    void forgetAndClearDropState() {
        KeyedRateLimiter k = limiter();
        k.tryAcquireAt(0L, "a", 3L);
        k.tryAcquireAt(0L, "b", 3L);
        assertTrue(k.forget("a"));
        assertFalse(k.forget("a"), "forgetting twice is not an error");
        assertEquals(1, k.size());
        assertInstanceOf(RateLimiter.Acquire.Ok.class, k.tryAcquireAt(0L, "a", 3L));
        k.clear();
        assertTrue(k.isEmpty());
    }

    @Test
    void configAccessorsRoundTrip() {
        KeyedRateLimiter k = new KeyedRateLimiter(2000.0, 8);
        assertEquals(2000.0, k.ratePerSec(), 1.0);
        assertEquals(8L, k.burstCapacity());
        assertTrue(k.nowNs() < 1_000_000_000L, "the clock starts at the origin");
        // The wall-clock entry points, not just the driven-time ones.
        assertTrue(k.tryAcquire("acct-a"));
        assertTrue(k.tryAcquire("acct-a", 2L));
        assertEquals(1, k.size());
    }

    @Test
    void concurrentKeysDoNotDoubleSpend() throws InterruptedException {
        // Driven time keeps the budget exact: nothing refills, so 4 keys with a
        // burst of 50 is 200 grants and not one more.
        KeyedRateLimiter k = new KeyedRateLimiter(1000.0, 50);
        AtomicInteger granted = new AtomicInteger();
        Thread[] ts = new Thread[8];
        for (int i = 0; i < ts.length; i++) {
            ts[i] = new Thread(() -> {
                for (int j = 0; j < 500; j++) {
                    if (k.tryAcquireAt(0L, "key-" + (j % 4), 1L) instanceof RateLimiter.Acquire.Ok) {
                        granted.incrementAndGet();
                    }
                }
            });
        }
        for (Thread t : ts) t.start();
        for (Thread t : ts) t.join();

        assertEquals(200, granted.get(), "4 keys x a burst of 50, no double-spend");
        assertEquals(4, k.size());
    }

    @Test
    void sweepingWhileAcquiringNeverLosesAGrant() throws InterruptedException {
        // The sweep unmaps keys under a concurrent acquire. If the acquire
        // wrote to the orphaned slot the grant would vanish from the
        // accounting, so the invariant is that grants per key never exceed the
        // burst between sweeps.
        KeyedRateLimiter k = new KeyedRateLimiter(1000.0, 4);
        AtomicInteger granted = new AtomicInteger();
        Thread sweeper = new Thread(() -> {
            for (int i = 0; i < 5000; i++) k.retainActiveAt(0L);
        });
        Thread acquirer = new Thread(() -> {
            for (int i = 0; i < 5000; i++) {
                if (k.tryAcquireAt(0L, "acct-a", 1L) instanceof RateLimiter.Acquire.Ok) {
                    granted.incrementAndGet();
                }
            }
        });
        sweeper.start();
        acquirer.start();
        sweeper.join();
        acquirer.join();

        // Every sweep at now=0 evicts nothing (a granted key sits at tat > 0),
        // so the burst of 4 is the whole budget regardless of interleaving.
        assertEquals(4, granted.get());
    }
}
