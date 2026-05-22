package com.submillisecond.primers.zgc;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.lang.management.MemoryMXBean;
import java.util.Arrays;
import java.util.List;
import java.util.Random;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Measure application-thread pause times under sustained allocation
 * pressure. Pauses are observed indirectly: a tight loop on one
 * platform thread reads {@code System.nanoTime()} repeatedly and
 * records the gap between successive reads. Anything bigger than the
 * loop's natural overhead is a thread suspension - GC safepoint,
 * scheduling jitter, or interrupt. The {@code max} value is the
 * worst pause the application would have observed.
 *
 * This is the same idea as Coordinated-Omission-aware tools
 * ({@code jHiccup}, {@code wrk2}, HdrHistogram). We do not parse GC
 * logs; we measure the effect on the application directly.
 *
 * Run the bench under each collector and compare:
 * <pre>
 *   java -XX:+UseZGC  -XX:+ZGenerational -Xms2g -Xmx2g \
 *        -cp target/classes com.submillisecond.primers.zgc.PauseTimeBench
 *   java -XX:+UseG1GC                    -Xms2g -Xmx2g \
 *        -cp target/classes com.submillisecond.primers.zgc.PauseTimeBench
 * </pre>
 * On a modern JVM you should see ZGC keep p99 hiccup under a few
 * hundred microseconds while G1 shows multi-millisecond young/mixed
 * collection pauses.
 *
 * Design notes:
 *   - Heartbeat thread is bound to a single platform thread (not a
 *     virtual thread) - a virtual thread parking/unparking blurs the
 *     pause measurement with scheduler interleaving on the carrier.
 *   - The allocator threads create both short-lived and survivor-aged
 *     objects so a generational collector has both phases to do.
 *   - We deliberately do not call System.gc(); we want the collector's
 *     own decisions, not a forced cycle.
 *   - Histogram is a sorted long[] of the heartbeat gap samples. For a
 *     5-second run at a sub-microsecond loop body we get tens of
 *     millions of samples; the array sort dominates wall time after
 *     the run, not during it, so it doesn't affect the measurement.
 */
public final class PauseTimeBench {
    private PauseTimeBench() {}

    private static final long WARMUP_SECONDS      = 2;
    private static final long RUN_SECONDS         = 5;
    private static final int  ALLOCATOR_THREADS   = 4;
    private static final int  SMALL_BYTES         = 1 << 10;   // 1 KiB
    private static final int  LARGE_BYTES         = 1 << 16;   // 64 KiB
    /** Approx number of heartbeat samples we keep; bigger -> more accurate tails, more memory. */
    private static final int  MAX_SAMPLES         = 50_000_000;

    public static void main(String[] args) throws InterruptedException {
        printBanner();

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
        ready.await();

        // Warmup window: allocator runs, JIT compiles the heartbeat loop and
        // the allocator path, the first GC cycles happen. Samples are
        // discarded. Otherwise the reported max is "JIT cold-start", which
        // tells you nothing about steady-state pause behaviour.
        long warmupStart = System.nanoTime();
        long resetSampleCount = measureHeartbeat(stop, WARMUP_SECONDS).length;
        long warmupMs = (System.nanoTime() - warmupStart) / 1_000_000;
        long preGcCollections = totalGcCollections();

        long[] gaps = measureHeartbeat(stop, RUN_SECONDS);

        long collectionsDuringRun = totalGcCollections() - preGcCollections;
        if (collectionsDuringRun == 0) {
            System.err.println("warning: no GC collections during the measurement window; allocator pressure too low");
        }
        System.out.printf("warmup:    %dms, %,d samples discarded%n%n", warmupMs, resetSampleCount);

        stop.set(true);
        for (Thread t : allocators) t.join(TimeUnit.SECONDS.toMillis(2));

        report(gaps, allocatedBytes.get());
    }

    private static void printBanner() {
        System.out.printf("JVM:        %s %s%n",
                System.getProperty("java.vm.name"), System.getProperty("java.runtime.version"));
        System.out.println("Collectors enabled (input args matter; default is G1):");
        for (GarbageCollectorMXBean gc : ManagementFactory.getGarbageCollectorMXBeans()) {
            System.out.printf("  - %s%n", gc.getName());
        }
        MemoryMXBean mem = ManagementFactory.getMemoryMXBean();
        long heapMaxMib = mem.getHeapMemoryUsage().getMax() / (1024 * 1024);
        System.out.printf("Heap max:   %d MiB%n", heapMaxMib);
        System.out.printf("Cores:      %d%n", Runtime.getRuntime().availableProcessors());
        System.out.println();
    }

    /**
     * Allocate a steady stream of garbage with a mix of short- and longer-lived
     * objects. We keep references in a small rolling buffer so survivors
     * actually promote into the old generation under generational collectors.
     */
    private static void allocLoop(AtomicBoolean stop, CountDownLatch ready,
                                  AtomicLong allocatedBytes, int seed) {
        Random rng = new Random(seed);
        // Rolling survivor buffer; objects in here stay live long enough to
        // be promoted under generational GC, but eventually rotate out.
        int rolling = 256;
        byte[][] survivors = new byte[rolling][];
        int idx = 0;
        ready.countDown();
        long localBytes = 0;
        while (!stop.get()) {
            int size = (rng.nextInt(8) == 0) ? LARGE_BYTES : SMALL_BYTES;
            byte[] b = new byte[size];
            // Touch the array so it isn't optimised away into a no-op.
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

    /**
     * Read nanoTime() in a tight loop for {@code seconds} and return a
     * compacted sample array of successive-read gaps in nanoseconds.
     *
     * The loop body is intentionally small but observable - the volatile
     * sink prevents the JIT from collapsing the call into a constant.
     */
    private static long[] measureHeartbeat(AtomicBoolean stop, long seconds) {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(seconds);
        long[] samples = new long[MAX_SAMPLES];
        int n = 0;

        long prev = System.nanoTime();
        long sink = 0;
        while (System.nanoTime() < deadline) {
            long now = System.nanoTime();
            long gap = now - prev;
            prev = now;
            if (n < samples.length) samples[n++] = gap;
            // Keep the JIT from eliding the loop body.
            sink ^= now;
        }
        SINK = sink;
        if (n < samples.length) samples = Arrays.copyOf(samples, n);
        return samples;
    }

    /** Black hole - assigning to a static volatile defeats dead-code elimination. */
    private static volatile long SINK = 0;

    private static void report(long[] gaps, long allocatedBytes) {
        if (gaps.length == 0) {
            System.out.println("no heartbeat samples - increase MAX_SAMPLES");
            return;
        }
        Arrays.sort(gaps);
        long p50  = gaps[(int) (gaps.length * 0.50)];
        long p99  = gaps[(int) (gaps.length * 0.99)];
        long p999 = gaps[(int) (gaps.length * 0.999)];
        long p9999= gaps[(int) (gaps.length * 0.9999)];
        long max  = gaps[gaps.length - 1];

        double allocatedMib = allocatedBytes / (1024.0 * 1024.0);
        double throughputMibPerSec = allocatedMib / RUN_SECONDS;

        System.out.println("Heartbeat gaps (lower is better; max is the worst observed STW pause):");
        System.out.printf("  samples = %,d%n", gaps.length);
        System.out.printf("  p50     = %s%n", formatNanos(p50));
        System.out.printf("  p99     = %s%n", formatNanos(p99));
        System.out.printf("  p99.9   = %s%n", formatNanos(p999));
        System.out.printf("  p99.99  = %s%n", formatNanos(p9999));
        System.out.printf("  max     = %s%n", formatNanos(max));
        System.out.println();
        System.out.printf("Allocator: %.1f MiB total -> %.1f MiB/s sustained over %ds with %d threads%n",
                allocatedMib, throughputMibPerSec, RUN_SECONDS, ALLOCATOR_THREADS);
        System.out.println();

        // Per-collector cumulative stats from the JVM. Useful for sanity:
        // if Pause Young/Mixed shows non-zero collection count we know the
        // collector actually ran during the bench.
        System.out.println("GC summary:");
        List<GarbageCollectorMXBean> gcs = ManagementFactory.getGarbageCollectorMXBeans();
        for (GarbageCollectorMXBean gc : gcs) {
            System.out.printf("  %-30s collections=%d  cumulative_time=%dms%n",
                    gc.getName(), gc.getCollectionCount(), gc.getCollectionTime());
        }
    }

    private static long totalGcCollections() {
        long n = 0;
        for (GarbageCollectorMXBean gc : ManagementFactory.getGarbageCollectorMXBeans()) {
            n += Math.max(0, gc.getCollectionCount());
        }
        return n;
    }

    private static String formatNanos(long ns) {
        if (ns >= 1_000_000) return String.format("%7.3f ms", ns / 1e6);
        if (ns >= 1_000)     return String.format("%7.2f us", ns / 1e3);
        return String.format("%7d ns", ns);
    }
}
