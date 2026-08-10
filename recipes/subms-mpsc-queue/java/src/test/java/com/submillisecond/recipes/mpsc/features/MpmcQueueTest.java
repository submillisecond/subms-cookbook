package com.submillisecond.recipes.mpsc.features;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MpmcQueueTest {

    @Test
    void singleThreadEnqueueDequeue() {
        MpmcQueue<Integer> q = new MpmcQueue<>(4);
        assertTrue(q.tryEnqueue(7));
        assertEquals(7, q.tryDequeue());
        assertNull(q.tryDequeue());
    }

    @Test
    void fullRingReturnsFalse() {
        MpmcQueue<Integer> q = new MpmcQueue<>(2);
        assertTrue(q.tryEnqueue(1));
        assertTrue(q.tryEnqueue(2));
        assertFalse(q.tryEnqueue(3));
    }

    @Test
    void fifoSingleProducerSingleConsumer() {
        MpmcQueue<Integer> q = new MpmcQueue<>(64);
        for (int i = 0; i < 64; i++) q.tryEnqueue(i);
        for (int i = 0; i < 64; i++) assertEquals(i, q.tryDequeue());
    }

    @Test
    void multiConsumerDrainsAllItemsExactlyOnce() throws InterruptedException {
        int producers = 4;
        int consumers = 4;
        int perProducer = 2_500;
        MpmcQueue<Long> q = new MpmcQueue<>(1024);

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
        AtomicInteger totalConsumed = new AtomicInteger();
        Thread[] cons = new Thread[consumers];
        AtomicInteger[] localCounts = new AtomicInteger[consumers];
        for (int c = 0; c < consumers; c++) {
            final int ci = c;
            localCounts[c] = new AtomicInteger();
            cons[c] = new Thread(() -> {
                while (true) {
                    Long v = q.tryDequeue();
                    if (v != null) {
                        localCounts[ci].incrementAndGet();
                        totalConsumed.incrementAndGet();
                    } else if (totalConsumed.get() >= total) {
                        break;
                    } else {
                        Thread.onSpinWait();
                    }
                }
            });
            cons[c].start();
        }

        for (Thread p : prods) p.join();
        for (Thread c : cons) c.join();
        assertEquals(total, totalConsumed.get());
        int sum = 0;
        for (AtomicInteger lc : localCounts) sum += lc.get();
        assertEquals(total, sum);
    }

    @Test
    void casRetriesNonZeroUnderContention() throws InterruptedException {
        int producers = 8;
        int perProducer = 5_000;
        MpmcQueue<Integer> q = new MpmcQueue<>(2048);
        Thread[] prods = new Thread[producers];
        for (int t = 0; t < producers; t++) {
            prods[t] = new Thread(() -> {
                int i = 0;
                while (i < perProducer) {
                    if (q.tryEnqueue(i)) i++;
                }
            });
            prods[t].start();
        }
        int total = producers * perProducer;
        Thread consumer = new Thread(() -> {
            int got = 0;
            while (got < total) {
                if (q.tryDequeue() != null) got++;
                else Thread.onSpinWait();
            }
        });
        consumer.start();
        for (Thread p : prods) p.join();
        consumer.join();
        // With 8 producers hammering one tail, contention is guaranteed.
        assertTrue(q.casRetries() > 0, "expected non-zero retries, got " + q.casRetries());
    }

    @Test
    void drainThenRefillWrapsRing() {
        MpmcQueue<Integer> q = new MpmcQueue<>(4);
        for (int round = 0; round < 10; round++) {
            for (int i = 0; i < 4; i++) assertTrue(q.tryEnqueue(round * 4 + i));
            for (int i = 0; i < 4; i++) assertEquals(round * 4 + i, q.tryDequeue());
        }
    }

    @Test
    void nullRejected() {
        MpmcQueue<Integer> q = new MpmcQueue<>(4);
        assertThrows(NullPointerException.class, () -> q.tryEnqueue(null));
    }

    @Test
    void sizeAndIsEmptyTrack() {
        MpmcQueue<Integer> q = new MpmcQueue<>(8);
        assertTrue(q.isEmpty());
        q.tryEnqueue(1);
        q.tryEnqueue(2);
        assertEquals(2, q.size());
        q.tryDequeue();
        assertEquals(1, q.size());
    }

    @Test
    void clearEmptiesTheRing() {
        MpmcQueue<Integer> q = new MpmcQueue<>(8);
        assertEquals(0, q.clear());
        for (int i = 0; i < 6; i++) q.tryEnqueue(i);
        assertEquals(6, q.clear());
        assertTrue(q.isEmpty());
        assertTrue(q.tryEnqueue(1));
    }

    @Test
    void isFullFlipsOnTheLastSlot() {
        MpmcQueue<Integer> q = new MpmcQueue<>(2);
        assertFalse(q.isFull());
        q.tryEnqueue(1);
        q.tryEnqueue(2);
        assertTrue(q.isFull());
        assertFalse(q.tryEnqueue(3));
        assertEquals(1, q.tryDequeue());
        assertFalse(q.isFull());
    }

    @Test
    void indicesAreMonotonicAndGiveLag() {
        MpmcQueue<Integer> q = new MpmcQueue<>(8);
        assertEquals(0, q.currentProducerIndex());
        assertEquals(0, q.currentConsumerIndex());
        for (int i = 0; i < 4; i++) q.tryEnqueue(i);
        assertEquals(4, q.currentProducerIndex());
        q.tryDequeue();
        assertEquals(1, q.currentConsumerIndex());
        assertEquals(q.size(), q.currentProducerIndex() - q.currentConsumerIndex());
    }
}
