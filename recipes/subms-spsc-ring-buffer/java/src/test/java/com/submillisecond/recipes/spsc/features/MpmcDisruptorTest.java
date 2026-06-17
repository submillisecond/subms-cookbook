package com.submillisecond.recipes.spsc.features;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MpmcDisruptorTest {

    @Test
    void singleProducerSingleConsumerRoundTrip() {
        MpmcDisruptor<Integer> d = new MpmcDisruptor<>(8, 1);
        MpmcDisruptor<Integer>.Producer p = d.producer();
        MpmcDisruptor<Integer>.Consumer c = d.consumer(0);
        assertTrue(p.tryPublish(1));
        assertTrue(p.tryPublish(2));
        assertEquals(1, c.tryConsume());
        assertEquals(2, c.tryConsume());
        assertNull(c.tryConsume());
    }

    @Test
    void twoConsumersBothSeeEveryItem() {
        MpmcDisruptor<Integer> d = new MpmcDisruptor<>(8, 2);
        MpmcDisruptor<Integer>.Producer p = d.producer();
        MpmcDisruptor<Integer>.Consumer c1 = d.consumer(0);
        MpmcDisruptor<Integer>.Consumer c2 = d.consumer(1);
        for (int i = 0; i < 5; i++) p.tryPublish(i);
        for (int i = 0; i < 5; i++) {
            assertEquals(i, c1.tryConsume());
            assertEquals(i, c2.tryConsume());
        }
        assertNull(c1.tryConsume());
        assertNull(c2.tryConsume());
    }

    @Test
    void producerBlocksWhenSlowestConsumerLags() {
        MpmcDisruptor<Integer> d = new MpmcDisruptor<>(2, 1);
        MpmcDisruptor<Integer>.Producer p = d.producer();
        MpmcDisruptor<Integer>.Consumer c = d.consumer(0);
        for (int i = 0; i < 2; i++) p.tryPublish(i);
        assertFalse(p.tryPublish(99));
        assertEquals(0, c.tryConsume());
        assertTrue(p.tryPublish(99));
    }

    @Test
    void twoProducersOneConsumerUnderThreads() throws InterruptedException {
        MpmcDisruptor<Long> d = new MpmcDisruptor<>(256, 1);
        MpmcDisruptor<Long>.Producer p1 = d.producer();
        MpmcDisruptor<Long>.Producer p2 = d.producer();
        long perProducer = 20_000L;
        AtomicLong count = new AtomicLong();
        long target = perProducer * 2;

        Thread t1 = new Thread(() -> {
            for (long i = 0; i < perProducer; i++) {
                long v = i;
                while (!p1.tryPublish(v)) Thread.onSpinWait();
            }
        });
        Thread t2 = new Thread(() -> {
            for (long i = 0; i < perProducer; i++) {
                long v = 1_000_000L + i;
                while (!p2.tryPublish(v)) Thread.onSpinWait();
            }
        });
        Thread consumer = new Thread(() -> {
            MpmcDisruptor<Long>.Consumer c = d.consumer(0);
            long local = 0;
            while (local < target) {
                if (c.tryConsume() != null) {
                    local++;
                    count.incrementAndGet();
                }
            }
        });
        t1.start(); t2.start(); consumer.start();
        t1.join(); t2.join(); consumer.join();
        assertEquals(target, count.get());
    }

    @Test
    void tryConsumeReturnsNullOnEmpty() {
        MpmcDisruptor<Integer> d = new MpmcDisruptor<>(4, 1);
        assertNull(d.consumer(0).tryConsume());
    }

    @Test
    void capacityRoundedUpToPowerOfTwo() {
        assertEquals(8, new MpmcDisruptor<Integer>(5, 1).capacity());
        assertEquals(2, new MpmcDisruptor<Integer>(1, 1).capacity());
    }

    @Test
    void invalidArgumentsThrow() {
        assertThrows(IllegalArgumentException.class, () -> new MpmcDisruptor<Integer>(4, 0));
        MpmcDisruptor<Integer> d = new MpmcDisruptor<>(4, 1);
        assertThrows(IndexOutOfBoundsException.class, () -> d.consumer(2));
        assertThrows(NullPointerException.class, () -> d.producer().tryPublish(null));
    }

    @Test
    void wrapsAroundCorrectlyAcrossRounds() {
        MpmcDisruptor<Integer> d = new MpmcDisruptor<>(4, 1);
        MpmcDisruptor<Integer>.Producer p = d.producer();
        MpmcDisruptor<Integer>.Consumer c = d.consumer(0);
        for (int round = 0; round < 4; round++) {
            for (int i = 0; i < 4; i++) p.tryPublish(round * 4 + i);
            for (int i = 0; i < 4; i++) assertEquals(round * 4 + i, c.tryConsume());
        }
    }
}
