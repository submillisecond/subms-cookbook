package com.submillisecond.recipes.mpsc.features;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicIntegerArray;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class BoundedMpscQueueTest {

    @Test
    void enqueueDequeueSingleItem() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(4);
        assertTrue(q.tryEnqueue(42));
        assertEquals(42, q.tryDequeue());
        assertNull(q.tryDequeue());
    }

    @Test
    void capacityIsPowerOfTwo() {
        assertEquals(8, new BoundedMpscQueue<Integer>(5).capacity());
        assertEquals(2, new BoundedMpscQueue<Integer>(1).capacity());
        assertEquals(16, new BoundedMpscQueue<Integer>(16).capacity());
    }

    @Test
    void enqueueFullReturnsFalse() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(4);
        for (int i = 0; i < 4; i++) {
            assertTrue(q.tryEnqueue(i));
        }
        assertFalse(q.tryEnqueue(99));
    }

    @Test
    void fifoOrderSingleProducer() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(16);
        for (int i = 0; i < 16; i++) {
            assertTrue(q.tryEnqueue(i));
        }
        for (int i = 0; i < 16; i++) {
            assertEquals(i, q.tryDequeue());
        }
    }

    @Test
    void drainThenRefillWrapsRing() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(4);
        for (int round = 0; round < 10; round++) {
            for (int i = 0; i < 4; i++) {
                assertTrue(q.tryEnqueue(round * 4 + i));
            }
            for (int i = 0; i < 4; i++) {
                assertEquals(round * 4 + i, q.tryDequeue());
            }
        }
    }

    @Test
    void multiProducerNoLostItems() throws InterruptedException {
        int producers = 4;
        int perProducer = 5_000;
        BoundedMpscQueue<Long> q = new BoundedMpscQueue<>(1024);
        AtomicIntegerArray counts = new AtomicIntegerArray(producers);
        AtomicInteger consumed = new AtomicInteger();

        Thread[] prods = new Thread[producers];
        for (int t = 0; t < producers; t++) {
            final long tid = t;
            prods[t] = new Thread(() -> {
                long i = 0;
                while (i < perProducer) {
                    if (q.tryEnqueue((tid << 32) | i)) {
                        i++;
                    } else {
                        Thread.onSpinWait();
                    }
                }
            });
            prods[t].start();
        }

        int total = producers * perProducer;
        Thread consumer = new Thread(() -> {
            int got = 0;
            while (got < total) {
                Long v = q.tryDequeue();
                if (v != null) {
                    counts.incrementAndGet((int) (v >>> 32));
                    got++;
                } else {
                    Thread.onSpinWait();
                }
            }
            consumed.set(got);
        });
        consumer.start();

        for (Thread p : prods) p.join();
        consumer.join();
        assertEquals(total, consumed.get());
        for (int t = 0; t < producers; t++) {
            assertEquals(perProducer, counts.get(t));
        }
    }

    @Test
    void sizeTracksOutstanding() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(8);
        assertTrue(q.isEmpty());
        q.tryEnqueue(1);
        q.tryEnqueue(2);
        assertEquals(2, q.size());
        q.tryDequeue();
        assertEquals(1, q.size());
    }

    @Test
    void nullValueRejected() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(4);
        assertThrows(NullPointerException.class, () -> q.tryEnqueue(null));
    }

    @Test
    void peekBorrowsTheNextSlot() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(4);
        assertNull(q.peek());
        q.tryEnqueue(5);
        q.tryEnqueue(6);
        assertEquals(5, q.peek());
        assertEquals(5, q.peek());
        assertEquals(5, q.tryDequeue());
        assertEquals(6, q.peek());
    }

    @Test
    void clearEmptiesTheRingAndReopensIt() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(4);
        assertEquals(0, q.clear());
        for (int i = 0; i < 4; i++) q.tryEnqueue(i);
        assertTrue(q.isFull());
        assertEquals(4, q.clear());
        assertTrue(q.isEmpty());
        assertFalse(q.isFull());
        assertTrue(q.tryEnqueue(99));
    }

    @Test
    void isFullFlipsOnTheLastSlot() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(2);
        assertFalse(q.isFull());
        q.tryEnqueue(1);
        assertFalse(q.isFull());
        q.tryEnqueue(2);
        assertTrue(q.isFull());
        assertEquals(1, q.tryDequeue());
        assertFalse(q.isFull());
    }

    @Test
    void indicesAreMonotonicAndGiveLag() {
        BoundedMpscQueue<Integer> q = new BoundedMpscQueue<>(8);
        assertEquals(0, q.currentProducerIndex());
        assertEquals(0, q.currentConsumerIndex());
        for (int i = 0; i < 5; i++) q.tryEnqueue(i);
        assertEquals(5, q.currentProducerIndex());
        assertEquals(0, q.currentConsumerIndex());
        assertEquals(q.size(), q.currentProducerIndex() - q.currentConsumerIndex());
        q.tryDequeue();
        q.tryDequeue();
        assertEquals(2, q.currentConsumerIndex());
        assertEquals(3, q.currentProducerIndex() - q.currentConsumerIndex());
    }
}
