package com.submillisecond.recipes.hdrhist.features;

import org.junit.jupiter.api.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.*;

final class DualRecorderTest {

    @Test
    void emptyDrainIsZero() {
        DualRecorder rec = new DualRecorder(3);
        Snapshot s = rec.getIntervalHistogram();
        assertEquals(0, s.count());
        assertEquals(0, s.valueAtPercentile(0.99));
    }

    @Test
    void drainReturnsRecordsSinceLastRotate() {
        DualRecorder rec = new DualRecorder(3);
        for (long i = 1; i <= 100; i++) rec.record(i);
        Snapshot s = rec.getIntervalHistogram();
        assertEquals(100, s.count());
        Snapshot empty = rec.getIntervalHistogram();
        assertEquals(0, empty.count());
    }

    @Test
    void rotationSwapsActiveSide() {
        DualRecorder rec = new DualRecorder(3);
        int first = rec.activeIndex();
        rec.record(10);
        rec.getIntervalHistogram();
        int second = rec.activeIndex();
        assertNotEquals(first, second);
    }

    @Test
    void recordsAfterRotateGoToNewSide() {
        DualRecorder rec = new DualRecorder(3);
        for (long i = 1; i <= 50; i++) rec.record(i);
        Snapshot first = rec.getIntervalHistogram();
        for (long i = 1; i <= 10; i++) rec.record(i * 100);
        Snapshot second = rec.getIntervalHistogram();
        assertEquals(50, first.count());
        assertEquals(10, second.count());
        assertTrue(second.max() >= 1000);
    }

    @Test
    void concurrentWritersWithPeriodicDrain() throws Exception {
        DualRecorder rec = new DualRecorder(3);
        int producers = 6;
        long perProducer = 20_000L;
        AtomicBoolean stop = new AtomicBoolean(false);
        AtomicLong produced = new AtomicLong(0);
        CountDownLatch start = new CountDownLatch(1);
        CountDownLatch producersDone = new CountDownLatch(producers);

        for (int t = 0; t < producers; t++) {
            final int tid = t;
            new Thread(() -> {
                try {
                    start.await();
                    for (long i = 0; i < perProducer; i++) {
                        rec.record((((long) tid * perProducer) + i) % 1000 + 1);
                        produced.incrementAndGet();
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                } finally {
                    producersDone.countDown();
                }
            }).start();
        }

        AtomicLong drained = new AtomicLong(0);
        Thread drainer = new Thread(() -> {
            while (!stop.get()) {
                Snapshot s = rec.getIntervalHistogram();
                drained.addAndGet(s.count());
                Thread.yield();
            }
            // Final drain.
            Snapshot s = rec.getIntervalHistogram();
            drained.addAndGet(s.count());
        });
        drainer.start();

        start.countDown();
        producersDone.await();
        stop.set(true);
        drainer.join();

        assertEquals(produced.get(), drained.get(),
                "every record() must show up in some snapshot");
    }

    @Test
    void backToBackDrainAlternatesSides() {
        DualRecorder rec = new DualRecorder(3);
        int i0 = rec.activeIndex();
        rec.getIntervalHistogram();
        int i1 = rec.activeIndex();
        rec.getIntervalHistogram();
        int i2 = rec.activeIndex();
        assertNotEquals(i0, i1);
        assertEquals(i0, i2);
    }
}
