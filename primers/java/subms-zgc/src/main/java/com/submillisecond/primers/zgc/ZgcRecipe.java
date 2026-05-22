package com.submillisecond.primers.zgc;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.util.Random;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/**
 * Stage: {@code heartbeat_gap_ns} - successive {@code System.nanoTime()} gaps
 * measured on one platform thread while four allocator threads churn garbage.
 * Any gap above the natural loop overhead is a thread suspension (GC
 * safepoint, scheduling jitter, page fault). Record {@code entries} samples.
 *
 * <p>Run with {@code -XX:+UseZGC -XX:+ZGenerational} (or any other collector)
 * to compare; the recipe itself doesn't pin the GC choice.
 */
public final class ZgcRecipe implements SubMsRecipe {

    private static final int ALLOCATOR_THREADS = 4;
    private static final int SMALL_BYTES = 1 << 10;
    private static final int LARGE_BYTES = 1 << 16;

    @Override public String name() { return "zgc-pause"; }

    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int samples = Math.max(10_000, params.entries());
        int warmupSamples = Math.max(1_000, params.warmup());

        CountDownLatch ready = new CountDownLatch(ALLOCATOR_THREADS);
        AtomicBoolean stop = new AtomicBoolean(false);
        AtomicLong allocatedBytes = new AtomicLong();
        Thread[] allocators = new Thread[ALLOCATOR_THREADS];
        for (int i = 0; i < ALLOCATOR_THREADS; i++) {
            final int seed = i;
            Thread t = new Thread(() -> allocLoop(stop, ready, allocatedBytes, seed),
                    "allocator-" + i);
            t.setDaemon(true);
            allocators[i] = t;
            t.start();
        }
        try { ready.await(); } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }

        long preGcCollections = totalGcCollections();

        // Warmup: discard.
        captureHeartbeat(warmupSamples, null);

        SubMsPerfHarness.Stage stage = h.stage("heartbeat_gap_ns", samples);
        captureHeartbeat(samples, stage);

        long collectionsDuringRun = totalGcCollections() - preGcCollections;

        stop.set(true);
        for (Thread t : allocators) {
            try { t.join(2000); } catch (InterruptedException ignored) {}
        }

        h.meta("allocator_threads", Integer.toString(ALLOCATOR_THREADS));
        h.meta("allocated_mib", String.format("%.1f", allocatedBytes.get() / (1024.0 * 1024.0)));
        h.meta("gc_collections", Long.toString(collectionsDuringRun));
        StringBuilder collectors = new StringBuilder();
        for (GarbageCollectorMXBean gc : ManagementFactory.getGarbageCollectorMXBeans()) {
            if (collectors.length() > 0) collectors.append(',');
            collectors.append(gc.getName());
        }
        h.meta("collectors", collectors.toString());
        h.meta("jvm", System.getProperty("java.vm.name") + " " + System.getProperty("java.runtime.version"));
    }

    /** Records `samples` successive nanoTime gaps. */
    private static void captureHeartbeat(int samples, SubMsPerfHarness.Stage stage) {
        long sink = 0;
        long prev = System.nanoTime();
        for (int i = 0; i < samples; i++) {
            long now = System.nanoTime();
            long gap = now - prev;
            prev = now;
            if (stage != null) stage.record(gap);
            sink ^= now;
        }
        SINK = sink;
    }

    private static volatile long SINK = 0;

    private static void allocLoop(AtomicBoolean stop, CountDownLatch ready,
                                  AtomicLong allocatedBytes, int seed) {
        Random rng = new Random(seed);
        int rolling = 256;
        byte[][] survivors = new byte[rolling][];
        int idx = 0;
        ready.countDown();
        long localBytes = 0;
        while (!stop.get()) {
            int size = (rng.nextInt(8) == 0) ? LARGE_BYTES : SMALL_BYTES;
            byte[] b = new byte[size];
            b[0] = (byte) size;
            b[size - 1] = (byte) (size >>> 8);
            survivors[idx] = b;
            idx = (idx + 1) % rolling;
            localBytes += size;
            if ((localBytes & ((1L << 24) - 1)) == 0) {
                allocatedBytes.addAndGet(localBytes);
                localBytes = 0;
            }
        }
        allocatedBytes.addAndGet(localBytes);
    }

    private static long totalGcCollections() {
        long n = 0;
        for (GarbageCollectorMXBean gc : ManagementFactory.getGarbageCollectorMXBeans()) {
            n += Math.max(0, gc.getCollectionCount());
        }
        return n;
    }
}
