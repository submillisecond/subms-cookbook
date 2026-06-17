package com.submillisecond.recipes.mpsc.features;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicIntegerArray;

import static org.junit.jupiter.api.Assertions.assertEquals;

final class BatchMpscQueueTest {

    @Test
    void batchDrainsUpToBufferSize() {
        BatchMpscQueue<Integer> q = new BatchMpscQueue<>();
        for (int i = 0; i < 10; i++) q.push(i);
        Integer[] buf = new Integer[4];
        int n = q.tryDequeueBatch(buf);
        assertEquals(4, n);
        for (int i = 0; i < n; i++) {
            assertEquals(i, buf[i]);
        }
    }

    @Test
    void batchStopsAtEmpty() {
        BatchMpscQueue<Integer> q = new BatchMpscQueue<>();
        q.push(1);
        q.push(2);
        Integer[] buf = new Integer[10];
        int n = q.tryDequeueBatch(buf);
        assertEquals(2, n);
        assertEquals(1, buf[0]);
        assertEquals(2, buf[1]);
        int n2 = q.tryDequeueBatch(buf);
        assertEquals(0, n2);
    }

    @Test
    void batchPreservesFifoOrder() {
        BatchMpscQueue<Integer> q = new BatchMpscQueue<>();
        for (int i = 0; i < 100; i++) q.push(i);
        Integer[] buf = new Integer[100];
        int n = q.tryDequeueBatch(buf);
        assertEquals(100, n);
        for (int i = 0; i < 100; i++) assertEquals(i, buf[i]);
    }

    @Test
    void batchWithCapLimit() {
        BatchMpscQueue<Integer> q = new BatchMpscQueue<>();
        for (int i = 0; i < 20; i++) q.push(i);
        Integer[] buf = new Integer[100];
        int n = q.tryDequeueBatch(buf, 5);
        assertEquals(5, n);
        for (int i = 0; i < 5; i++) assertEquals(i, buf[i]);
    }

    @Test
    void multiProducerBatchDrainLosesNothing() throws InterruptedException {
        int producers = 4;
        int perProducer = 10_000;
        BatchMpscQueue<Long> q = new BatchMpscQueue<>();

        Thread[] prods = new Thread[producers];
        for (int t = 0; t < producers; t++) {
            final long tid = t;
            prods[t] = new Thread(() -> {
                for (long i = 0; i < perProducer; i++) {
                    q.push((tid << 32) | i);
                }
            });
            prods[t].start();
        }

        int total = producers * perProducer;
        AtomicIntegerArray counts = new AtomicIntegerArray(producers);
        Thread consumer = new Thread(() -> {
            Long[] buf = new Long[256];
            int got = 0;
            while (got < total) {
                int n = q.tryDequeueBatch(buf);
                for (int i = 0; i < n; i++) {
                    counts.incrementAndGet((int) (buf[i] >>> 32));
                    got++;
                }
                if (n == 0) Thread.onSpinWait();
            }
        });
        consumer.start();
        for (Thread p : prods) p.join();
        consumer.join();
        for (int t = 0; t < producers; t++) {
            assertEquals(perProducer, counts.get(t));
        }
    }

    @Test
    void zeroLengthBufferReturnsZero() {
        BatchMpscQueue<Integer> q = new BatchMpscQueue<>();
        q.push(1);
        Integer[] buf = new Integer[0];
        assertEquals(0, q.tryDequeueBatch(buf));
    }
}
