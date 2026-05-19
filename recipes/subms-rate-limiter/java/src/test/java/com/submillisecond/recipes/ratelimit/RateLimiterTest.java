package com.submillisecond.recipes.ratelimit;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
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
}
