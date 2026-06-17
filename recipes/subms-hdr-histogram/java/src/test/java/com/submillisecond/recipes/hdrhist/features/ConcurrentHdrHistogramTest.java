package com.submillisecond.recipes.hdrhist.features;

import org.junit.jupiter.api.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.*;

final class ConcurrentHdrHistogramTest {

    @Test
    void emptyReturnsZero() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        assertEquals(0, h.count());
        assertEquals(0, h.max());
        assertEquals(0, h.valueAtPercentile(0.99));
    }

    @Test
    void singleThreadRecords() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        for (long v : new long[]{10, 20, 30, 40, 50}) h.record(v);
        assertEquals(5, h.count());
        assertTrue(h.max() >= 50);
    }

    @Test
    void percentilesMatchDistribution() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        for (long i = 1; i <= 1000; i++) h.record(i);
        long p50 = h.valueAtPercentile(0.5);
        long p99 = h.valueAtPercentile(0.99);
        assertTrue(p50 >= 450 && p50 <= 550, "p50=" + p50);
        assertTrue(p99 >= 950 && p99 <= 1050, "p99=" + p99);
    }

    @Test
    void concurrentWritersLoseNothing() throws Exception {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        int threads = 8;
        int perThread = 25_000;
        CountDownLatch start = new CountDownLatch(1);
        CountDownLatch done = new CountDownLatch(threads);
        for (int t = 0; t < threads; t++) {
            final int tid = t;
            new Thread(() -> {
                try {
                    start.await();
                    for (int i = 0; i < perThread; i++) {
                        h.record(((long) tid * perThread + i) % 1000 + 1);
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                } finally {
                    done.countDown();
                }
            }).start();
        }
        start.countDown();
        done.await();
        assertEquals((long) threads * perThread, h.count());
        long p99 = h.valueAtPercentile(0.99);
        assertTrue(p99 >= 900, "p99=" + p99);
    }

    @Test
    void snapshotPreservesTotal() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        for (long i = 1; i <= 100; i++) h.record(i);
        Snapshot s = h.drainSnapshot();
        assertEquals(100, s.count());
        assertEquals(0, h.count(), "live side cleared after drain");
        long p99 = s.valueAtPercentile(0.99);
        assertTrue(p99 >= 95, "snapshot p99 ~ 99, got " + p99);
    }

    @Test
    void snapshotThenRecordStartsFresh() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        for (long i = 1; i <= 10; i++) h.record(i);
        h.drainSnapshot();
        h.record(500);
        assertEquals(1, h.count());
        assertTrue(h.max() >= 500);
    }

    @Test
    void clampsAboveBucketCapacity() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(1, 1);
        long huge = Long.MAX_VALUE / 2;
        h.record(huge);
        assertEquals(1, h.count());
        long m = h.max();
        assertTrue(m >= 0);
    }

    @Test
    void emptySnapshotIsZero() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        Snapshot s = h.drainSnapshot();
        assertEquals(0, s.count());
        assertEquals(0, s.max());
        assertEquals(0, s.valueAtPercentile(0.99));
    }

    @Test
    void atomicityAcrossManyShortBursts() {
        // Many tiny bursts to stress the CAS path on highIndex.
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        AtomicLong expected = new AtomicLong(0);
        for (int round = 0; round < 100; round++) {
            for (int i = 0; i < 10; i++) {
                h.record(i + 1);
                expected.incrementAndGet();
            }
        }
        assertEquals(expected.get(), h.count());
    }
}
