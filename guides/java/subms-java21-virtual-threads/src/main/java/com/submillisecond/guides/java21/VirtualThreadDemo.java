package com.submillisecond.guides.java21;

import java.time.Duration;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.LockSupport;

/**
 * The minimum thing that shows a virtual thread is doing work.
 *
 * Spawns 100,000 virtual threads; each parks for 10 ms and increments a
 * counter. A platform-thread implementation of the same shape would
 * either run out of memory or take ~100x longer through a fixed pool.
 *
 * The interesting line is `Thread.ofVirtual().start(...)`. Everything
 * else is just counting and timing.
 */
public final class VirtualThreadDemo {
    private VirtualThreadDemo() {}

    private static final int    TASKS    = 100_000;
    private static final long   PARK_MS  = 10;

    public static void main(String[] args) throws InterruptedException {
        AtomicLong done = new AtomicLong();
        Thread[] threads = new Thread[TASKS];

        long t0 = System.nanoTime();
        for (int i = 0; i < TASKS; i++) {
            threads[i] = Thread.ofVirtual().start(() -> {
                LockSupport.parkNanos(Duration.ofMillis(PARK_MS).toNanos());
                done.incrementAndGet();
            });
        }
        for (Thread t : threads) t.join();
        long elapsedMs = (System.nanoTime() - t0) / 1_000_000;

        System.out.printf("vthreads:  %d  done:%d  wall:%dms  (each parked %dms)%n",
                TASKS, done.get(), elapsedMs, PARK_MS);
        if (done.get() != TASKS) {
            throw new AssertionError("expected " + TASKS + " completions, got " + done.get());
        }
        // 100k virtual threads each parking 10ms should land well under 1 second
        // of wall time on any modern laptop. Platform threads through a fixed
        // pool would serialise this into pool-size batches.
        if (elapsedMs > 5_000) {
            throw new AssertionError("wall time " + elapsedMs + "ms unexpectedly slow");
        }
    }
}
