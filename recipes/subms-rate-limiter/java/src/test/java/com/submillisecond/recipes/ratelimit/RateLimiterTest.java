package com.submillisecond.recipes.ratelimit;

import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class RateLimiterTest {

    @Test
    void allowsABurst() {
        RateLimiter rl = new RateLimiter(100.0, 10);
        int got = 0;
        for (int i = 0; i < 20; i++) if (rl.tryAcquire()) got++;
        assertTrue(got >= 10 && got <= 11, "expected ~burst permits, got " + got);
    }

    @Test
    void refillsOverTime() throws InterruptedException {
        RateLimiter rl = new RateLimiter(1000.0, 5);
        for (int i = 0; i < 6; i++) rl.tryAcquire();
        assertFalse(rl.tryAcquire(), "no refill yet");
        Thread.sleep(5);
        assertTrue(rl.tryAcquire(), "should refill after wait");
    }

    @Test
    void rateAndBurstAccessors() {
        RateLimiter rl = new RateLimiter(2000.0, 8);
        assertEquals(2000.0, rl.ratePerSec(), 1.0);
        assertEquals(8L, rl.burstCapacity());
    }

    @Test
    void concurrentAcquiresDontDoubleSpend() throws InterruptedException {
        RateLimiter rl = new RateLimiter(10_000.0, 100);
        AtomicInteger granted = new AtomicInteger();
        int threads = 8;
        int attempts = 1000;

        Thread[] ts = new Thread[threads];
        for (int i = 0; i < threads; i++) {
            ts[i] = new Thread(() -> {
                for (int j = 0; j < attempts; j++) {
                    if (rl.tryAcquire()) granted.incrementAndGet();
                }
            });
        }
        for (Thread t : ts) t.start();
        for (Thread t : ts) t.join();

        int g = granted.get();
        assertTrue(g >= 100, "burst should grant >=100, got " + g);
        assertTrue(g < threads * attempts, "rate should reject most, got " + g);
    }

    @Test
    void rejectsWhenDrainedImmediately() {
        RateLimiter rl = new RateLimiter(1.0, 1);
        assertTrue(rl.tryAcquire());
        assertFalse(rl.tryAcquire());
        assertFalse(rl.tryAcquire());
    }

    @Test
    void highRateLowBurstGrantsSteadily() {
        RateLimiter rl = new RateLimiter(100_000.0, 1);
        int got = 0;
        for (int i = 0; i < 1000; i++) if (rl.tryAcquire()) got++;
        assertTrue(got >= 1, "burst should grant at least 1, got " + got);
    }

    @Test
    void oneThreadSteadyState() {
        RateLimiter rl = new RateLimiter(50_000.0, 100);
        int total = 0;
        for (int i = 0; i < 10_000; i++) if (rl.tryAcquire()) total++;
        assertTrue(total >= 100, "burst at minimum; got " + total);
    }

    @Test
    void zeroBurstCapacityIsFlooredToOne() {
        // A window of zero would reject the very first request. Both ports
        // floor the capacity at one permit instead.
        RateLimiter rl = new RateLimiter(1000.0, 0);
        assertEquals(1L, rl.burstCapacity());
        assertTrue(rl.tryAcquireAt(0L), "the floored window admits one permit");
        assertFalse(rl.tryAcquireAt(0L), "and only one");
    }

    @Test
    void drivenTimeRefillsExactly() {
        // 1000/sec => period 1ms; burst 5 => window 5ms. Driving `now` pins the
        // refill boundary instead of sleeping on it.
        RateLimiter rl = new RateLimiter(1000.0, 5);
        for (int i = 0; i < 5; i++) assertTrue(rl.tryAcquireAt(0L), "the burst drains at t=0");
        assertFalse(rl.tryAcquireAt(0L), "the 6th permit overshoots the window");
        assertTrue(rl.tryAcquireAt(1_000_000L), "one period refills one permit");
        assertFalse(rl.tryAcquireAt(1_500_000L), "half a period refills nothing");
        assertTrue(rl.tryAcquireAt(2_000_000L));
    }

    @Test
    void weightedDrawCostsNPeriods() {
        RateLimiter rl = new RateLimiter(1000.0, 5);
        assertTrue(rl.tryAcquireAt(0L, 3L), "a weight-3 message fits the window");
        assertTrue(rl.tryAcquireAt(0L, 2L), "the remaining 2 fit exactly");
        assertFalse(rl.tryAcquireAt(0L, 1L), "the window is spent");
        assertTrue(rl.tryAcquireAt(3_000_000L, 3L), "three periods buy back three permits");
    }

    @Test
    void aRejectedWeightedDrawSpendsNothing() {
        RateLimiter rl = new RateLimiter(1000.0, 5);
        assertTrue(rl.tryAcquireAt(0L, 4L));
        assertFalse(rl.tryAcquireAt(0L, 3L), "4 + 3 overshoots a burst of 5");
        assertTrue(rl.tryAcquireAt(0L, 1L),
                "the rejected batch must not have spent the remaining permit");
    }

    @Test
    void weightAboveBurstIsTypedAsUnattainable() {
        RateLimiter rl = new RateLimiter(1000.0, 5);
        RateLimiter.Acquire outcome = rl.tryAcquireWithRetryAt(0L, 6L);
        RateLimiter.Acquire.Unattainable u =
                assertInstanceOf(RateLimiter.Acquire.Unattainable.class, outcome);
        assertEquals(5L, u.burstCapacity());
        assertTrue(rl.timeUntilReadyAt(0L, 6L).isEmpty());
        assertTrue(rl.tryAcquireAt(0L, 5L), "the limiter is untouched by it");
    }

    @Test
    void zeroWeightIsAFreeProbe() {
        RateLimiter rl = new RateLimiter(1000.0, 2);
        assertInstanceOf(RateLimiter.Acquire.Ok.class, rl.tryAcquireWithRetryAt(0L, 0L));
        assertEquals(Duration.ZERO, rl.timeUntilReadyAt(0L, 0L).orElseThrow());
        assertTrue(rl.tryAcquireAt(0L, 2L), "the probe advanced nothing");
    }

    @Test
    void negativeWeightIsRejectedLoudly() {
        RateLimiter rl = new RateLimiter(1000.0, 2);
        assertThrows(IllegalArgumentException.class, () -> rl.tryAcquireWithRetryAt(0L, -1L));
    }

    @Test
    void timeUntilReadyReadsWithoutSpending() {
        RateLimiter rl = new RateLimiter(1000.0, 2); // period 1ms, window 2ms
        assertEquals(Duration.ZERO, rl.timeUntilReadyAt(0L, 1L).orElseThrow());
        assertEquals(Duration.ZERO, rl.timeUntilReadyAt(0L, 1L).orElseThrow(),
                "a peek must not consume the permit it reports on");
        assertTrue(rl.tryAcquireAt(0L, 2L));
        assertEquals(Duration.ofNanos(1_000_000L), rl.timeUntilReadyAt(0L, 1L).orElseThrow());
        // The peek agrees with what the mutating call reports.
        RateLimiter.Acquire.Retry r = assertInstanceOf(
                RateLimiter.Acquire.Retry.class, rl.tryAcquireWithRetryAt(0L));
        assertEquals(Duration.ofNanos(1_000_000L), r.retryAfter());
        assertEquals(Duration.ZERO, rl.timeUntilReadyAt(1_000_000L, 1L).orElseThrow());
    }

    @Test
    void timeUntilReadyOnTheInternalClock() {
        RateLimiter rl = new RateLimiter(1.0, 1); // one permit per second
        assertEquals(Duration.ZERO, rl.timeUntilReady(1L).orElseThrow());
        assertTrue(rl.tryAcquire(1L));
        assertTrue(rl.timeUntilReady(1L).orElseThrow().toNanos() > 0,
                "a spent limiter reports a positive wait");
    }

    @Test
    void resetReturnsTheFullBurst() {
        RateLimiter rl = new RateLimiter(1000.0, 3);
        for (int i = 0; i < 3; i++) assertTrue(rl.tryAcquireAt(0L));
        assertFalse(rl.tryAcquireAt(0L));
        rl.reset();
        for (int i = 0; i < 3; i++) {
            assertTrue(rl.tryAcquireAt(0L), "reset restores the whole burst");
        }
        assertFalse(rl.tryAcquireAt(0L));
    }

    @Test
    void acquireWithinReturnsImmediatelyWhenPermitted() throws InterruptedException {
        RateLimiter rl = new RateLimiter(1000.0, 4);
        long started = System.nanoTime();
        assertTrue(rl.acquireWithin(1L, Duration.ofSeconds(5)));
        assertTrue(System.nanoTime() - started < 500_000_000L, "an available permit must not sleep");
    }

    @Test
    void acquireWithinGivesUpRatherThanSleepingPastTheDeadline() throws InterruptedException {
        // period 1s, burst 1: the second permit is a second away.
        RateLimiter rl = new RateLimiter(1.0, 1);
        assertTrue(rl.acquireWithin(1L, Duration.ofMillis(1)));
        long started = System.nanoTime();
        assertFalse(rl.acquireWithin(1L, Duration.ofMillis(1)),
                "a 1s wait cannot be satisfied inside a 1ms timeout");
        assertTrue(System.nanoTime() - started < 500_000_000L, "it must refuse without sleeping");
        // An unattainable weight is refused on the spot, not waited on.
        assertFalse(rl.acquireWithin(9L, Duration.ofSeconds(30)));
    }

    @Test
    void acquireWithinSleepsThenSucceeds() throws InterruptedException {
        // period 2ms, burst 1. The second call waits ~2ms, comfortably inside
        // the 5s timeout even on a loaded box.
        RateLimiter rl = new RateLimiter(500.0, 1);
        assertTrue(rl.acquireWithin(1L, Duration.ofSeconds(5)));
        assertTrue(rl.acquireWithin(1L, Duration.ofSeconds(5)));
    }

    @Test
    void nowNsTracksTheLimitersOwnOrigin() {
        RateLimiter rl = new RateLimiter(1000.0, 4);
        long a = rl.nowNs();
        long b = rl.nowNs();
        assertTrue(b >= a, "the clock is monotonic");
        assertTrue(a < 1_000_000_000L, "and starts at the limiter's origin");
        assertTrue(rl.tryAcquireAt(rl.nowNs()));
    }

    @Test
    void contendedDrivenTimeAcquiresDoNotDoubleSpend() throws InterruptedException {
        // Every thread passes the SAME `now`, so the only thing that can
        // separate them is the CAS - which makes this the test that exercises
        // the retry arm of the CAS loops rather than just their happy path.
        // The burst is sized to the whole workload so no attempt takes the
        // early reject exit and every one of them races.
        int threads = 8;
        int attempts = 2000;
        int budget = threads * attempts;
        RateLimiter forBool = new RateLimiter(1_000_000.0, budget);
        RateLimiter forTyped = new RateLimiter(1_000_000.0, budget);
        // The wall-clock entry point takes the same race. Elapsed time only
        // ever adds permits, so a burst sized to the workload still means every
        // attempt is granted and the expected count is exact.
        RateLimiter forWallClock = new RateLimiter(1_000_000.0, budget);
        AtomicInteger boolGrants = new AtomicInteger();
        AtomicInteger typedGrants = new AtomicInteger();
        AtomicInteger wallGrants = new AtomicInteger();

        Thread[] ts = new Thread[threads];
        for (int i = 0; i < ts.length; i++) {
            ts[i] = new Thread(() -> {
                for (int j = 0; j < attempts; j++) {
                    if (forBool.tryAcquireAt(0L)) boolGrants.incrementAndGet();
                    if (forTyped.tryAcquireWithRetryAt(0L, 1L) instanceof RateLimiter.Acquire.Ok) {
                        typedGrants.incrementAndGet();
                    }
                    if (forWallClock.tryAcquire()) wallGrants.incrementAndGet();
                }
            });
        }
        for (Thread t : ts) t.start();
        for (Thread t : ts) t.join();

        // A frozen clock means no refill: the burst is the exact budget, so a
        // lost CAS that granted anyway would show up as a count above it.
        assertEquals(budget, boolGrants.get());
        assertEquals(budget, typedGrants.get());
        assertEquals(budget, wallGrants.get());
        assertFalse(forBool.tryAcquireAt(0L), "the budget is spent to the permit");
        assertFalse(forTyped.tryAcquireAt(0L), "the budget is spent to the permit");
    }

    @Test
    void veryHighRateDoesNotOverflow() {
        RateLimiter rl = new RateLimiter(1_000_000_000.0, 1000);
        assertEquals(1000L, rl.burstCapacity());
        for (int i = 0; i < 100; i++) assertTrue(rl.tryAcquire());
    }

    @Test
    void ratePerSecRoundTripsAfterRounding() {
        // 1000 permits/sec => period = 1_000_000 ns/permit. ratePerSec()
        // computes 1e9 / period; should return ~1000 within rounding.
        RateLimiter rl = new RateLimiter(1000.0, 10);
        double observed = rl.ratePerSec();
        assertTrue(observed >= 999.0 && observed <= 1001.0,
                "ratePerSec round-trip within +-1: " + observed);
    }

    @Test
    void retryReportsOkWhileUnderLimit() {
        RateLimiter rl = new RateLimiter(1.0, 5);
        for (int i = 0; i < 5; i++) {
            assertInstanceOf(RateLimiter.Acquire.Ok.class, rl.tryAcquireWithRetry());
        }
    }

    @Test
    void retryReportsWaitWhenExhausted() {
        RateLimiter rl = new RateLimiter(1.0, 5); // period 1s, burst 5
        for (int i = 0; i < 5; i++) rl.tryAcquireWithRetry();
        RateLimiter.Acquire r = rl.tryAcquireWithRetry();
        assertInstanceOf(RateLimiter.Acquire.Retry.class, r);
        Duration wait = ((RateLimiter.Acquire.Retry) r).retryAfter();
        assertTrue(wait.toNanos() > 0, "a rejected request must wait a positive time");
        // Wait is bounded by one token period (1ms); elapsed real time shrinks it.
        assertTrue(wait.compareTo(Duration.ofSeconds(1)) <= 0,
                "wait bounded by the token period: " + wait);
    }

    @Test
    void retryRejectionDoesNotAdvanceTheLimiter() {
        RateLimiter rl = new RateLimiter(1.0, 2);
        rl.tryAcquireWithRetry();
        rl.tryAcquireWithRetry();
        Duration first = ((RateLimiter.Acquire.Retry) rl.tryAcquireWithRetry()).retryAfter();
        Duration second = ((RateLimiter.Acquire.Retry) rl.tryAcquireWithRetry()).retryAfter();
        // A rejection leaves tat untouched, so the wait never grows across repeated
        // rejections - only elapsed real time shrinks it.
        assertTrue(second.compareTo(first) <= 0,
                "rejection must not advance tat: " + first + " then " + second);
    }

    @Test
    void retryOkAgreesWithTryAcquire() {
        RateLimiter rl = new RateLimiter(1.0, 3);
        assertTrue(rl.tryAcquire());
        assertInstanceOf(RateLimiter.Acquire.Ok.class, rl.tryAcquireWithRetry());
        assertTrue(rl.tryAcquire());
        assertFalse(rl.tryAcquire(), "burst of 3 is exhausted");
        assertInstanceOf(RateLimiter.Acquire.Retry.class, rl.tryAcquireWithRetry());
    }
}
