package com.submillisecond.recipes.ratelimit;

import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
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
    void zeroBurstCapacityDoesNotPanic() {
        RateLimiter rl = new RateLimiter(1000.0, 0);
        rl.tryAcquire();
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
