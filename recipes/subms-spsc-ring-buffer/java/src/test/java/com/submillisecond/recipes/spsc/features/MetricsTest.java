package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;
import com.submillisecond.recipes.spsc.features.Metrics.Instrumented;
import com.submillisecond.recipes.spsc.features.Metrics.RingMetrics;
import com.submillisecond.recipes.spsc.features.Metrics.RingMetricsSnapshot;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MetricsTest {

    @Test
    void countsEnqueueSuccessAndFail() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(2);
        Instrumented<Integer> ins = new Instrumented<>(q.producer(), q.consumer());
        assertTrue(ins.producer.tryPush(1));
        assertTrue(ins.producer.tryPush(2));
        assertFalse(ins.producer.tryPush(3));
        RingMetricsSnapshot s = ins.metrics.snapshot();
        assertEquals(2, s.enqueueSuccess);
        assertEquals(1, s.enqueueFail);
    }

    @Test
    void countsDequeueSuccessAndFail() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        Instrumented<Integer> ins = new Instrumented<>(q.producer(), q.consumer());
        ins.producer.tryPush(7);
        assertEquals(7, ins.consumer.tryPop());
        ins.consumer.tryPop();
        ins.consumer.tryPop();
        RingMetricsSnapshot s = ins.metrics.snapshot();
        assertEquals(1, s.dequeueSuccess);
        assertEquals(2, s.dequeueFail);
    }

    @Test
    void tracksMaxDepthObserved() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        Instrumented<Integer> ins = new Instrumented<>(q.producer(), q.consumer());
        for (int i = 0; i < 4; i++) ins.producer.tryPush(i);
        RingMetricsSnapshot s = ins.metrics.snapshot();
        assertEquals(4, s.maxDepthObserved);
    }

    @Test
    void snapshotInitiallyZeros() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        Instrumented<Integer> ins = new Instrumented<>(q.producer(), q.consumer());
        RingMetricsSnapshot s = ins.metrics.snapshot();
        assertEquals(0, s.enqueueSuccess);
        assertEquals(0, s.enqueueFail);
        assertEquals(0, s.dequeueSuccess);
        assertEquals(0, s.dequeueFail);
        assertEquals(0, s.maxDepthObserved);
        assertEquals(0, s.casRetries);
    }

    @Test
    void casRetryCounterIsWritable() {
        RingMetrics m = new RingMetrics();
        for (int i = 0; i < 7; i++) m.recordCasRetry();
        assertEquals(7, m.snapshot().casRetries);
    }

    @Test
    void metricsObservedAcrossThreads() throws InterruptedException {
        SpscRingBuffer<Long> q = new SpscRingBuffer<>(64);
        Instrumented<Long> ins = new Instrumented<>(q.producer(), q.consumer());
        long n = 10_000L;
        Thread producer = new Thread(() -> {
            for (long i = 0; i < n; i++) {
                while (!ins.producer.tryPush(i)) Thread.onSpinWait();
            }
        });
        Thread consumer = new Thread(() -> {
            long got = 0;
            while (got < n) {
                if (ins.consumer.tryPop() != null) got++;
            }
        });
        producer.start(); consumer.start();
        producer.join(); consumer.join();
        RingMetricsSnapshot s = ins.metrics.snapshot();
        assertEquals(n, s.enqueueSuccess);
        assertEquals(n, s.dequeueSuccess);
    }
}
