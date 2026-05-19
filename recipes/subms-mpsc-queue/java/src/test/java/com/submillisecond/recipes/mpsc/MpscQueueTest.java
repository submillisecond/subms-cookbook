package com.submillisecond.recipes.mpsc;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class MpscQueueTest {

    private static <T> T pollEventually(MpscQueue<T> q) {
        for (int i = 0; i < 10_000_000; i++) {
            T v = q.tryPoll();
            if (v != null) return v;
            if (!q.isInconsistent()) return null;
            Thread.onSpinWait();
        }
        throw new AssertionError("stuck inconsistent");
    }

    @Test
    void pushAndPopASingleValue() {
        MpscQueue<Integer> q = new MpscQueue<>();
        q.push(7);
        assertEquals(7, pollEventually(q));
        assertNull(q.tryPoll());
    }

    @Test
    void fifoOrderSingleProducer() {
        MpscQueue<Integer> q = new MpscQueue<>();
        for (int i = 0; i < 100; i++) q.push(i);
        for (int i = 0; i < 100; i++) assertEquals(i, pollEventually(q));
    }

    @Test
    void rejectsNullValues() {
        MpscQueue<Integer> q = new MpscQueue<>();
        assertThrows(NullPointerException.class, () -> q.push(null));
    }

    @Test
    void multiProducerNoLostItems() throws InterruptedException {
        int producers = 4;
        int perProducer = 50_000;
        MpscQueue<Long> q = new MpscQueue<>();
        AtomicInteger total = new AtomicInteger();
        int[] counts = new int[producers];

        Thread[] ps = new Thread[producers];
        for (int tid = 0; tid < producers; tid++) {
            final int t = tid;
            ps[tid] = new Thread(() -> {
                for (int i = 0; i < perProducer; i++) {
                    q.push(((long) t << 32) | i);
                }
            });
        }

        Thread consumer = new Thread(() -> {
            int got = 0;
            int target = producers * perProducer;
            while (got < target) {
                Long v = q.tryPoll();
                if (v != null) {
                    counts[(int) (v >>> 32)]++;
                    got++;
                    total.incrementAndGet();
                } else {
                    Thread.onSpinWait();
                }
            }
        });

        for (Thread p : ps) p.start();
        consumer.start();
        for (Thread p : ps) p.join();
        consumer.join();

        assertEquals(producers * perProducer, total.get());
        for (int c : counts) assertEquals(perProducer, c);
    }

    @Test
    void isInconsistentTrueOnlyDuringPublishGap() {
        MpscQueue<Integer> q = new MpscQueue<>();
        // Empty queue: not inconsistent.
        assertNull(q.tryPoll());
        assertEquals(false, q.isInconsistent());
        // After a push, eventually drains cleanly.
        q.push(1);
        assertEquals(1, pollEventually(q));
        assertEquals(false, q.isInconsistent());
    }

    @Test
    void alternatingPushPoll() {
        MpscQueue<Integer> q = new MpscQueue<>();
        for (int i = 0; i < 1000; i++) {
            q.push(i);
            assertEquals(i, pollEventually(q));
        }
    }

    @Test
    void drainThenRefill() {
        MpscQueue<Integer> q = new MpscQueue<>();
        for (int i = 0; i < 50; i++) q.push(i);
        for (int i = 0; i < 50; i++) assertEquals(i, pollEventually(q));
        assertNull(pollEventually(q));
        for (int i = 100; i < 120; i++) q.push(i);
        for (int i = 100; i < 120; i++) assertEquals(i, pollEventually(q));
    }

    @Test
    void higherProducerContention() throws InterruptedException {
        int producers = 8;
        int perProducer = 25_000;
        MpscQueue<Long> q = new MpscQueue<>();
        AtomicInteger total = new AtomicInteger();

        Thread[] ps = new Thread[producers];
        for (int tid = 0; tid < producers; tid++) {
            final int t = tid;
            ps[tid] = new Thread(() -> {
                for (int i = 0; i < perProducer; i++) {
                    q.push(((long) t << 32) | i);
                }
            });
        }
        Thread consumer = new Thread(() -> {
            int got = 0;
            int target = producers * perProducer;
            while (got < target) {
                Long v = q.tryPoll();
                if (v != null) { got++; total.incrementAndGet(); }
                else { Thread.onSpinWait(); }
            }
        });
        for (Thread p : ps) p.start();
        consumer.start();
        for (Thread p : ps) p.join();
        consumer.join();
        assertEquals(producers * perProducer, total.get());
    }

    @Test
    void largeSingleThreadWorkload() {
        MpscQueue<Long> q = new MpscQueue<>();
        long n = 100_000L;
        for (long i = 0; i < n; i++) q.push(i);
        long next = 0;
        while (next < n) {
            Long v = q.tryPoll();
            if (v != null) {
                assertEquals(next, v);
                next++;
            } else if (!q.isInconsistent()) {
                break;
            }
        }
        assertEquals(n, next);
    }

    @Test
    void popOnlyReturnsExactlyOnce() {
        MpscQueue<Integer> q = new MpscQueue<>();
        q.push(1);
        assertEquals(1, pollEventually(q));
        // Once drained, repeated polls all return null.
        for (int i = 0; i < 10; i++) assertNull(pollEventually(q));
    }
}
