package com.submillisecond.recipes.spsc;

import com.submillisecond.recipes.spsc.features.Bulk;
import com.submillisecond.recipes.spsc.features.Metrics;
import com.submillisecond.recipes.spsc.features.MpmcDisruptor;
import com.submillisecond.recipes.spsc.features.MpscFanIn;
import com.submillisecond.recipes.spsc.features.WaitStrategies;
import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(64);
        SpscRingBuffer<Integer>.Producer tx = q.producer();
        SpscRingBuffer<Integer>.Consumer rx = q.consumer();
        assertTrue(tx.tryPush(42));            // wait-free push; false only when full
        assertEquals(42, rx.tryPop());         // wait-free pop; null only when empty
        assertNull(rx.tryPop());               // empty ring
        // quickstart:end
    }

    @Test
    void baseDropsOnFullAndPreservesOrder() throws InterruptedException {
        SpscRingBuffer<Long> small = new SpscRingBuffer<>(4);
        SpscRingBuffer<Long>.Producer stx = small.producer();
        int dropped = 0;
        for (long seq = 0; seq < 6; seq++) {
            if (!stx.tryPush(seq)) dropped++;
        }
        assertEquals(2, dropped, "two ticks past capacity are dropped, not blocked");
        SpscRingBuffer<Long>.Consumer srx = small.consumer();
        assertTrue(stx.isFull());
        assertEquals(4, srx.size());
        assertEquals(0L, srx.peek());
        assertEquals(4, srx.clear());

        long n = 20_000;
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();
        Thread feed = new Thread(() -> {
            for (long seq = 0; seq < n; seq++) {
                while (!tx.tryPush(seq)) Thread.onSpinWait();
            }
        });
        AtomicLong received = new AtomicLong();
        Thread strategy = new Thread(() -> {
            long expected = 0;
            while (expected < n) {
                Long tick = rx.tryPop();
                if (tick != null) {
                    assertEquals(expected, tick, "ticks arrive in feed order");
                    expected++;
                }
            }
            received.set(expected);
        });
        feed.start();
        strategy.start();
        feed.join();
        strategy.join();
        assertEquals(n, received.get());
    }

    @Test
    void bulkBatchRoundTripsInOrder() {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(16);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();
        Long[] batch = new Long[10];
        for (int i = 0; i < batch.length; i++) batch[i] = 20_000L + i;
        assertEquals(10, Bulk.tryEnqueueBulk(tx, batch));

        Long[] out = new Long[10];
        assertEquals(10, Bulk.tryDequeueBulk(rx, out));
        for (int i = 0; i < out.length; i++) {
            assertEquals(batch[i], out[i]);
        }
    }

    @Test
    void blockingHandoffPreservesOrder() throws InterruptedException {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(8);
        WaitStrategies.BlockingSpscProducer<Long> producer =
                new WaitStrategies.BlockingSpscProducer<>(ring.producer(), new WaitStrategies.YieldStrategy());
        WaitStrategies.BlockingSpscConsumer<Long> consumer =
                new WaitStrategies.BlockingSpscConsumer<>(ring.consumer(), new WaitStrategies.YieldStrategy());
        long n = 5_000;
        Thread feed = new Thread(() -> {
            for (long seq = 0; seq < n; seq++) producer.push(seq);
        });
        Thread strategy = new Thread(() -> {
            for (long expected = 0; expected < n; expected++) {
                assertEquals(expected, consumer.pop());
            }
        });
        feed.start();
        strategy.start();
        feed.join();
        strategy.join();
    }

    @Test
    void fanInDrainsEveryVenue() throws InterruptedException {
        int venues = 3;
        long perVenue = 10_000;
        MpscFanIn<Long> fanIn = new MpscFanIn<>(venues, 256);
        MpscFanIn<Long>.Consumer consumer = fanIn.consumer();
        Thread[] feeds = new Thread[venues];
        for (int v = 0; v < venues; v++) {
            MpscFanIn<Long>.Producer p = fanIn.producer(v);
            feeds[v] = new Thread(() -> {
                for (long seq = 0; seq < perVenue; seq++) {
                    while (!p.tryPush(seq)) Thread.onSpinWait();
                }
            });
        }
        long total = perVenue * venues;
        AtomicLong got = new AtomicLong();
        Thread strategy = new Thread(() -> {
            long local = 0;
            while (local < total) {
                if (consumer.tryPop() != null) local++;
            }
            got.set(local);
        });
        for (Thread f : feeds) f.start();
        strategy.start();
        for (Thread f : feeds) f.join();
        strategy.join();
        assertEquals(total, got.get());
    }

    @Test
    void disruptorBroadcastsEveryTickToBothReaders() {
        long n = 8;
        MpmcDisruptor<Long> disruptor = new MpmcDisruptor<>(16, 2);
        MpmcDisruptor<Long>.Producer producer = disruptor.producer();
        List<MpmcDisruptor<Long>.Consumer> consumers = disruptor.consumers();
        MpmcDisruptor<Long>.Consumer strategy = consumers.get(0);
        MpmcDisruptor<Long>.Consumer risk = consumers.get(1);

        long published = 0, stratSeen = 0, riskSeen = 0;
        while (published < n) {
            while (published < n && producer.tryPublish(published)) published++;
            Long v;
            while ((v = strategy.tryConsume()) != null) {
                assertEquals(stratSeen, v);
                stratSeen++;
            }
            while ((v = risk.tryConsume()) != null) {
                assertEquals(riskSeen, v);
                riskSeen++;
            }
        }
        assertEquals(n, stratSeen);
        assertEquals(n, riskSeen);
    }

    @Test
    void metricsCountSuccessFailAndPeakDepth() {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(4);
        Metrics.Instrumented<Long> inst = new Metrics.Instrumented<>(ring.producer(), ring.consumer());
        for (long seq = 0; seq < 4; seq++) {
            assertTrue(inst.producer.tryPush(seq));
        }
        assertFalse(inst.producer.tryPush(99L));
        for (int i = 0; i < 4; i++) {
            assertNotNull(inst.consumer.tryPop());
        }
        assertNull(inst.consumer.tryPop());

        Metrics.RingMetricsSnapshot snap = inst.metrics.snapshot();
        assertEquals(4, snap.enqueueSuccess);
        assertEquals(1, snap.enqueueFail);
        assertEquals(4, snap.dequeueSuccess);
        assertEquals(1, snap.dequeueFail);
        assertEquals(4, snap.maxDepthObserved);
    }
}
