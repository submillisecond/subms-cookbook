package com.submillisecond.guides.java21;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.locks.LockSupport;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/**
 * Stages: {@code vthread} and {@code platform_pool}. Both submit {@code entries}
 * tasks that each park for {@code PARK_MICROS}; per-task latency is recorded
 * from enqueue to completion. The pair captures the queue-wait + park cost
 * each scheduler imposes.
 */
public final class Java21Recipe implements SubMsRecipe {

    private static final long PARK_MICROS = 20_000;   // 20 ms per task
    private static final int  PLATFORM_POOL = 64;

    @Override public String name() { return "java21-virtual-threads"; }

    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int tasks = params.entries();
        int warmup = params.warmup();

        // Warm up the carrier pool / JIT - discarded.
        runWorkload(Executors.newVirtualThreadPerTaskExecutor(), Math.min(warmup, tasks), null);

        SubMsPerfHarness.Stage vthread = h.stage("vthread", tasks);
        long vtWall = runWorkload(Executors.newVirtualThreadPerTaskExecutor(), tasks, vthread);

        SubMsPerfHarness.Stage platform = h.stage("platform_pool", tasks);
        long platWall = runWorkload(
            Executors.newFixedThreadPool(PLATFORM_POOL, platformThreadFactory()),
            tasks, platform);

        h.meta("park_micros", Long.toString(PARK_MICROS));
        h.meta("platform_pool_size", Integer.toString(PLATFORM_POOL));
        h.meta("vthread_wall_ms", Long.toString(vtWall / 1_000_000));
        h.meta("platform_wall_ms", Long.toString(platWall / 1_000_000));
        if (vtWall > 0) {
            h.meta("wall_speedup", String.format("%.1f", (double) platWall / vtWall));
        }
    }

    /** Returns wall time in nanoseconds. If {@code stage} is non-null, records per-task latencies. */
    private static long runWorkload(ExecutorService exec, int tasks, SubMsPerfHarness.Stage stage) {
        long[] latency = new long[tasks];
        CountDownLatch done = new CountDownLatch(tasks);
        long parkNanos = PARK_MICROS * 1_000L;
        long wallStart = System.nanoTime();
        try {
            for (int i = 0; i < tasks; i++) {
                final int idx = i;
                final long submittedAt = System.nanoTime();
                exec.execute(() -> {
                    LockSupport.parkNanos(parkNanos);
                    latency[idx] = System.nanoTime() - submittedAt;
                    done.countDown();
                });
            }
            done.await();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        } finally {
            exec.shutdownNow();
        }
        long wall = System.nanoTime() - wallStart;
        if (stage != null) {
            for (long ns : latency) stage.record(ns);
        }
        return wall;
    }

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
