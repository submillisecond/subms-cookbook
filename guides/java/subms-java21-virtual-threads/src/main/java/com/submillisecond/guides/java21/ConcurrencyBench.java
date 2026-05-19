package com.submillisecond.guides.java21;

import java.util.Arrays;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.locks.LockSupport;

/**
 * Head-to-head wall-clock measurement: virtual threads vs a fixed
 * platform-thread pool, on an IO-bound workload of N independent tasks
 * each parking for a fixed amount of simulated work.
 *
 * Design notes:
 *
 *   - Flat parallelism, not fan-out. Submitting children to the same
 *     fixed pool that holds the parent is the classic thread-pool
 *     starvation deadlock; we sidestep it by submitting independent
 *     tasks directly, which is also the cleanest like-for-like shape.
 *
 *   - Park time is chosen above OS scheduler granularity. On Windows
 *     the default kernel timer resolution rounds short LockSupport
 *     parks up to ~15 ms; PARK_MICROS sits well clear of that so the
 *     measurement isn't dominated by timer slop.
 *
 *   - Warmup is a separate phase, discarded. Otherwise the
 *     platform-pool measurement absorbs JIT compilation of the worker
 *     loop and the vthread measurement gets the carrier pool warm "for
 *     free".
 *
 *   - We measure per-task latency from the moment the task is enqueued
 *     until the work returns - that is queue wait plus park - because
 *     that is the latency a downstream caller experiences.
 *
 *   - The wall-speedup assert is loose (>=10x) so a noisy CI runner
 *     does not flake the test. Typical observed margin on a quiet
 *     laptop is 50-200x for these parameters.
 */
public final class ConcurrencyBench {
    private ConcurrencyBench() {}

    private static final int  TASKS            = 10_000;
    private static final long PARK_MICROS      = 20_000;   // 20 ms / task, clear of Windows timer slop
    private static final int  PLATFORM_POOL    = 64;
    private static final int  WARMUP_TASKS     = 2_000;
    private static final int  MIN_WALL_SPEEDUP = 10;

    public static void main(String[] args) throws Exception {
        // Warmup - output discarded.
        runOnce("warmup-vthread",
                Executors::newVirtualThreadPerTaskExecutor, WARMUP_TASKS, false);
        runOnce("warmup-platform",
                () -> Executors.newFixedThreadPool(PLATFORM_POOL, platformThreadFactory()),
                WARMUP_TASKS, false);

        Result vthread  = runOnce("vthread",
                Executors::newVirtualThreadPerTaskExecutor, TASKS, true);
        Result platform = runOnce("platform pool=" + PLATFORM_POOL,
                () -> Executors.newFixedThreadPool(PLATFORM_POOL, platformThreadFactory()),
                TASKS, true);

        double speedup = (double) platform.wallMillis / vthread.wallMillis;
        System.out.println();
        System.out.printf("Speedup (platform / vthread): %.1fx wall%n", speedup);

        if (speedup < MIN_WALL_SPEEDUP) {
            throw new AssertionError(String.format(
                    "expected >= %dx wall speedup; got %.1fx (vthread=%dms, platform=%dms)",
                    MIN_WALL_SPEEDUP, speedup, vthread.wallMillis, platform.wallMillis));
        }

        long parkNanos = PARK_MICROS * 1_000L;
        if (vthread.p99Nanos > parkNanos * 4) {
            throw new AssertionError(String.format(
                    "vthread p99 task latency %.2fms exceeds 4x park budget (%.2fms)",
                    vthread.p99Nanos / 1e6, parkNanos / 1e6));
        }
    }

    private record Result(String label, long wallMillis,
                          long p50Nanos, long p99Nanos, long p999Nanos, long maxNanos) {}

    @FunctionalInterface
    private interface ExecutorSupplier { ExecutorService get(); }

    private static Result runOnce(String label, ExecutorSupplier execFactory,
                                  int tasks, boolean print) throws Exception {
        long[] latencyNanos = new long[tasks];
        CountDownLatch done = new CountDownLatch(tasks);
        long parkNanos = PARK_MICROS * 1_000L;

        long wallStart = System.nanoTime();
        ExecutorService exec = execFactory.get();
        try {
            for (int i = 0; i < tasks; i++) {
                final int idx = i;
                final long submittedAt = System.nanoTime();
                exec.execute(() -> {
                    LockSupport.parkNanos(parkNanos);
                    latencyNanos[idx] = System.nanoTime() - submittedAt;
                    done.countDown();
                });
            }
            done.await();
        } finally {
            // shutdownNow rather than close(): we already joined via the
            // latch, and we want to release worker threads immediately
            // without paying executor shutdown handshake cost in the wall
            // measurement.
            exec.shutdownNow();
        }
        long wallMillis = (System.nanoTime() - wallStart) / 1_000_000;

        long[] sorted = latencyNanos.clone();
        Arrays.sort(sorted);
        long p50  = sorted[(int) (sorted.length * 0.50)];
        long p99  = sorted[(int) (sorted.length * 0.99)];
        long p999 = sorted[(int) (sorted.length * 0.999)];
        long max  = sorted[sorted.length - 1];

        if (print) {
            System.out.printf("%-22s tasks=%d  park=%dus  wall=%dms  p50=%.2fms p99=%.2fms p999=%.2fms max=%.2fms%n",
                    label, tasks, PARK_MICROS, wallMillis,
                    p50 / 1e6, p99 / 1e6, p999 / 1e6, max / 1e6);
        }
        return new Result(label, wallMillis, p50, p99, p999, max);
    }

    /** Daemon, named platform threads so a leak doesn't keep the JVM alive. */
    private static ThreadFactory platformThreadFactory() {
        return new ThreadFactory() {
            private int n = 0;
            @Override public Thread newThread(Runnable r) {
                Thread t = new Thread(r, "bench-platform-" + (n++));
                t.setDaemon(true);
                return t;
            }
        };
    }
}
