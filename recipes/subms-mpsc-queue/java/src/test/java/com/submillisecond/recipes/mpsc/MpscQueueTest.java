package com.submillisecond.recipes.mpsc;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MpscQueueTest {

    /**
     * The queue must not retain what it has already handed out. {@code stub} is
     * a final field, so anything still linked from it stays reachable for the
     * queue's lifetime - and because {@link MpscQueue#size()} walks from
     * {@code tail}, it keeps reporting the correct live count while the heap
     * fills. That combination exhausted a 700 MB heap in the feature sweep after
     * ~25 M round trips.
     */
    @Test
    void drainedNodesAreUnlinkedSoTheQueueDoesNotRetainItsHistory() throws Exception {
        MpscQueue<Integer> q = new MpscQueue<>();
        for (int i = 0; i < 1_000; i++) q.push(i);
        for (int i = 0; i < 1_000; i++) assertEquals(i, q.tryPoll());
        assertEquals(0, q.size());

        java.lang.reflect.Field stubField = MpscQueue.class.getDeclaredField("stub");
        stubField.setAccessible(true);
        Object stub = stubField.get(q);
        java.lang.reflect.Field nextField = stub.getClass().getDeclaredField("next");
        nextField.setAccessible(true);
        assertNull(nextField.get(stub), "stub still links the drained chain");
    }

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

    @Test
    void peekBorrowsWithoutConsuming() {
        MpscQueue<Integer> q = new MpscQueue<>();
        assertNull(q.peek());
        q.push(11);
        q.push(22);
        assertEquals(11, q.peek());
        assertEquals(11, q.peek());
        assertEquals(11, pollEventually(q));
        assertEquals(22, q.peek());
        assertEquals(22, pollEventually(q));
        assertNull(q.peek());
    }

    @Test
    void isEmptyTracksTheDrain() {
        MpscQueue<Integer> q = new MpscQueue<>();
        assertTrue(q.isEmpty());
        q.push(1);
        assertFalse(q.isEmpty());
        assertEquals(1, pollEventually(q));
        assertTrue(q.isEmpty());
    }

    @Test
    void sizeCountsTheBacklog() {
        MpscQueue<Integer> q = new MpscQueue<>();
        assertEquals(0, q.size());
        for (int i = 0; i < 5; i++) q.push(i);
        assertEquals(5, q.size());
        pollEventually(q);
        assertEquals(4, q.size());
    }

    @Test
    void clearDrainsAndReportsTheCount() {
        MpscQueue<Integer> q = new MpscQueue<>();
        assertEquals(0, q.clear());
        for (int i = 0; i < 7; i++) q.push(i);
        assertEquals(7, q.clear());
        assertTrue(q.isEmpty());
        assertEquals(0, q.size());
        q.push(99);
        assertEquals(99, pollEventually(q));
    }

    @Test
    void pushBatchPublishesAWholeRunInOrder() {
        MpscQueue<Integer> q = new MpscQueue<>();
        assertEquals(0, q.pushBatch(new Integer[0]));
        assertTrue(q.isEmpty());

        assertEquals(3, q.pushBatch(new Integer[] {1, 2, 3}));
        assertEquals(3, q.size());
        for (int expected = 1; expected <= 3; expected++) {
            assertEquals(expected, pollEventually(q));
        }
        assertTrue(q.isEmpty());
    }

    @Test
    void pushBatchHonoursTheLengthAndRejectsNulls() {
        MpscQueue<Integer> q = new MpscQueue<>();
        Integer[] run = {1, 2, 3, 4};
        assertEquals(2, q.pushBatch(run, 2));
        assertEquals(2, q.size());
        assertEquals(4, q.pushBatch(run, 99), "a length past the array is clamped");
        assertThrows(NullPointerException.class, () -> q.pushBatch(new Integer[] {1, null}));
    }

    @Test
    void pushBatchInterleavesWithSinglePushes() {
        MpscQueue<Integer> q = new MpscQueue<>();
        q.push(0);
        q.pushBatch(new Integer[] {1, 2, 3});
        q.push(4);
        for (int expected = 0; expected <= 4; expected++) {
            assertEquals(expected, pollEventually(q));
        }
        assertTrue(q.isEmpty());
    }

    @Test
    void concurrentPushBatchLosesNothing() throws InterruptedException {
        final int producers = 4;
        final int runs = 250;
        final int runLen = 8;
        MpscQueue<Integer> q = new MpscQueue<>();

        Thread[] threads = new Thread[producers];
        for (int p = 0; p < producers; p++) {
            final int pid = p;
            threads[p] = new Thread(() -> {
                Integer[] run = new Integer[runLen];
                for (int r = 0; r < runs; r++) {
                    int base = pid * runs * runLen + r * runLen;
                    for (int i = 0; i < runLen; i++) run[i] = base + i;
                    q.pushBatch(run);
                }
            });
            threads[p].start();
        }
        for (Thread t : threads) t.join();

        int total = producers * runs * runLen;
        boolean[] seen = new boolean[total];
        int drained = 0;
        while (drained < total) {
            Integer v = q.tryPoll();
            if (v != null) {
                assertFalse(seen[v], "no item published twice");
                seen[v] = true;
                drained++;
            } else if (!q.isInconsistent()) {
                break;
            }
        }
        assertEquals(total, drained);
    }
}
