package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BlockingSpscConsumer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BlockingSpscProducer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BusySpin;
import com.submillisecond.recipes.spsc.features.WaitStrategies.ParkStrategy;
import com.submillisecond.recipes.spsc.features.WaitStrategies.YieldStrategy;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class WaitStrategiesTest {

    @Test
    void busySpinSingleThreadRoundTrip() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        BlockingSpscProducer<Integer> p = new BlockingSpscProducer<>(q.producer(), new BusySpin());
        BlockingSpscConsumer<Integer> c = new BlockingSpscConsumer<>(q.consumer(), new BusySpin());
        p.push(7);
        assertEquals(7, c.pop());
    }

    @Test
    void yieldStrategyHandlesFullThenDrains() throws InterruptedException {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        BlockingSpscProducer<Integer> p = new BlockingSpscProducer<>(q.producer(), new YieldStrategy());
        BlockingSpscConsumer<Integer> c = new BlockingSpscConsumer<>(q.consumer(), new YieldStrategy());
        Thread consumer = new Thread(() -> {
            for (int i = 0; i < 20; i++) {
                int v = c.pop();
                if (v != i) throw new AssertionError("out of order at " + i + ": " + v);
            }
        });
        consumer.start();
        for (int i = 0; i < 20; i++) p.push(i);
        consumer.join();
    }

    @Test
    void parkStrategyWakesBlockedConsumer() throws InterruptedException {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        ParkStrategy[] pair = ParkStrategy.pair();
        BlockingSpscProducer<Integer> p = new BlockingSpscProducer<>(q.producer(), pair[0]);
        BlockingSpscConsumer<Integer> c = new BlockingSpscConsumer<>(q.consumer(), pair[1]);

        Object[] result = new Object[1];
        Thread consumer = new Thread(() -> result[0] = c.pop());
        consumer.start();
        Thread.sleep(20);
        p.push(42);
        long t0 = System.nanoTime();
        consumer.join(2_000);
        long elapsedMs = (System.nanoTime() - t0) / 1_000_000;
        assertTrue(elapsedMs < 1_900, "consumer didn't join within budget: " + elapsedMs + "ms");
        assertEquals(42, result[0]);
    }

    @Test
    void parkStrategyWakesBlockedProducer() throws InterruptedException {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(2);
        ParkStrategy[] pair = ParkStrategy.pair();
        BlockingSpscProducer<Integer> p = new BlockingSpscProducer<>(q.producer(), pair[0]);
        BlockingSpscConsumer<Integer> c = new BlockingSpscConsumer<>(q.consumer(), pair[1]);
        p.push(1);
        p.push(2);
        // Producer thread will block on the third push until the consumer drains.
        Thread producer = new Thread(() -> {
            p.push(3);
            p.push(4);
        });
        producer.start();
        Thread.sleep(20);
        assertEquals(1, c.pop());
        assertEquals(2, c.pop());
        producer.join(2_000);
        assertEquals(3, c.pop());
        assertEquals(4, c.pop());
    }

    @Test
    void parkStrategyHandlesHighThroughputRoundTrip() throws InterruptedException {
        SpscRingBuffer<Long> q = new SpscRingBuffer<>(64);
        ParkStrategy[] pair = ParkStrategy.pair();
        BlockingSpscProducer<Long> p = new BlockingSpscProducer<>(q.producer(), pair[0]);
        BlockingSpscConsumer<Long> c = new BlockingSpscConsumer<>(q.consumer(), pair[1]);
        long n = 50_000L;
        Thread producer = new Thread(() -> { for (long i = 0; i < n; i++) p.push(i); });
        Thread consumer = new Thread(() -> {
            for (long i = 0; i < n; i++) {
                long v = c.pop();
                if (v != i) throw new AssertionError("out of order at " + i);
            }
        });
        producer.start();
        consumer.start();
        producer.join();
        consumer.join();
    }

    @Test
    void tryPushSucceedsWithoutBlocking() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        BlockingSpscProducer<Integer> p = new BlockingSpscProducer<>(q.producer(), new BusySpin());
        assertTrue(p.tryPush(1));
        assertTrue(p.tryPush(2));
    }
}
