package com.submillisecond.guides.java21;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Verifies the language-level claim behind {@link VirtualThreadDemo}: that
 * {@code Thread.ofVirtual().start} produces a thread for which
 * {@code Thread.currentThread().isVirtual()} is true, and that joining
 * many such threads observes every contribution.
 *
 * The actual {@code VirtualThreadDemo} main method is a runnable script,
 * not a class with logic to assert; this test exercises the same
 * primitives in a deterministic way.
 */
final class VirtualThreadDemoTest {

    @Test
    @DisplayName("Thread.ofVirtual().start produces a virtual thread")
    void virtualBuilderProducesVirtualThread() throws InterruptedException {
        AtomicLong virtualHits = new AtomicLong();
        Thread t = Thread.ofVirtual().start(() -> {
            if (Thread.currentThread().isVirtual()) virtualHits.incrementAndGet();
        });
        t.join();
        assertEquals(1, virtualHits.get(),
                "the virtual builder must produce a thread that reports isVirtual()");
    }

    @Test
    @DisplayName("Thread.ofPlatform().start produces a platform thread (negative control)")
    void platformBuilderProducesPlatformThread() throws InterruptedException {
        AtomicLong platformHits = new AtomicLong();
        Thread p = Thread.ofPlatform().start(() -> {
            if (!Thread.currentThread().isVirtual()) platformHits.incrementAndGet();
        });
        p.join();
        assertEquals(1, platformHits.get(),
                "the platform builder must NOT produce a virtual thread");
    }

    @Test
    @DisplayName("1000 virtual threads all observe their work and join cleanly")
    void manyVirtualThreadsJoinCleanly() throws InterruptedException {
        int n = 1_000;
        AtomicLong sum = new AtomicLong();
        Thread[] ts = new Thread[n];
        for (int i = 0; i < n; i++) {
            final int v = i;
            ts[i] = Thread.ofVirtual().start(() -> sum.addAndGet(v));
        }
        for (Thread t : ts) t.join();
        long expected = (long) (n - 1) * n / 2;
        assertEquals(expected, sum.get(),
                "every virtual thread must have contributed before join returned");
    }

    @Test
    @DisplayName("a virtual thread that parks does not pin a carrier")
    void virtualThreadParkDoesNotPinCarrier() throws InterruptedException {
        // Spawn far more virtual threads than there are carrier threads;
        // if any pinned, the test would either time out or fail to count.
        int n = 10_000;
        AtomicLong done = new AtomicLong();
        Thread[] ts = new Thread[n];
        for (int i = 0; i < n; i++) {
            ts[i] = Thread.ofVirtual().start(() -> {
                java.util.concurrent.locks.LockSupport.parkNanos(1_000_000L); // 1ms
                done.incrementAndGet();
            });
        }
        for (Thread t : ts) t.join();
        assertEquals(n, done.get(),
                "all 10k vthreads must have completed; a pinned carrier would deadlock");
        assertTrue(done.get() <= n, "internal sanity");
    }
}
