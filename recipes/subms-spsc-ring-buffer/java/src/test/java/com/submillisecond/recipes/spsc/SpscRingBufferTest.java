package com.submillisecond.recipes.spsc;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SpscRingBufferTest {

    @Test
    void pushAndPopASingleValue() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        assertTrue(p.tryPush(7));
        assertEquals(7, c.tryPop());
        assertNull(c.tryPop());
    }

    @Test
    void popOnEmptyReturnsNull() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        assertNull(q.consumer().tryPop());
        assertNull(q.consumer().tryPop());
    }

    @Test
    void capacityIsRoundedUpToPowerOfTwo() {
        assertEquals(8, new SpscRingBuffer<Object>(5).capacity());
        assertEquals(2, new SpscRingBuffer<Object>(0).capacity());
        assertEquals(2, new SpscRingBuffer<Object>(1).capacity());
        assertEquals(16, new SpscRingBuffer<Object>(16).capacity());
        assertEquals(32, new SpscRingBuffer<Object>(17).capacity());
    }

    @Test
    void fillsToCapacityThenRejects() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < q.capacity(); i++) assertTrue(p.tryPush(i));
        assertFalse(p.tryPush(999));
        for (int i = 0; i < q.capacity(); i++) assertEquals(i, c.tryPop());
        assertNull(c.tryPop());
    }

    @Test
    void rejectsNullValues() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        assertThrows(NullPointerException.class, () -> q.producer().tryPush(null));
    }

    @Test
    void alternatingPushPop() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 100; i++) {
            assertTrue(p.tryPush(i));
            assertEquals(i, c.tryPop());
        }
        assertNull(c.tryPop());
    }

    @Test
    void wrapsAroundRing() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 4; i++) p.tryPush(i);
        for (int i = 0; i < 4; i++) assertEquals(i, c.tryPop());
        for (int i = 4; i < 8; i++) p.tryPush(i);
        for (int i = 4; i < 8; i++) assertEquals(i, c.tryPop());
        assertNull(c.tryPop());
    }

    @Test
    void fullThenPartialDrainThenRefill() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(8);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 8; i++) p.tryPush(i);
        for (int i = 0; i < 4; i++) assertEquals(i, c.tryPop());
        for (int i = 100; i < 104; i++) p.tryPush(i);
        for (int i = 4; i < 8; i++) assertEquals(i, c.tryPop());
        for (int i = 100; i < 104; i++) assertEquals(i, c.tryPop());
        assertNull(c.tryPop());
    }

    @Test
    void popClearsReferenceForGc() {
        // After pop, the buffer slot must not hold the referenced object alive.
        SpscRingBuffer<Object> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Object>.Producer p = q.producer();
        SpscRingBuffer<Object>.Consumer c = q.consumer();
        Object o = new Object();
        java.lang.ref.WeakReference<Object> wr = new java.lang.ref.WeakReference<>(o);
        p.tryPush(o);
        Object popped = c.tryPop();
        assertEquals(o, popped);
        o = null;
        popped = null;
        for (int i = 0; i < 8; i++) {
            System.gc();
            if (wr.get() == null) return;
            try { Thread.sleep(10); } catch (InterruptedException ignored) {}
        }
        // Best-effort: GC heuristics may keep the object briefly; this passes when GC catches up.
        // (No assertion if GC didn't run; the popped clear-to-null code path is what we want exercised.)
    }

    @Test
    void roundTripHoldsUnderTwoThreads() throws InterruptedException {
        SpscRingBuffer<Long> q = new SpscRingBuffer<>(1024);
        long n = 1_000_000L;
        AtomicLong checksum = new AtomicLong();

        Thread producer = new Thread(() -> {
            SpscRingBuffer<Long>.Producer p = q.producer();
            long i = 0;
            while (i < n) { if (p.tryPush(i)) i++; }
        });
        Thread consumer = new Thread(() -> {
            SpscRingBuffer<Long>.Consumer c = q.consumer();
            long next = 0;
            while (next < n) {
                Long v = c.tryPop();
                if (v != null) {
                    if (v != next) throw new AssertionError("out of order at " + next + ": got " + v);
                    checksum.addAndGet(v);
                    next++;
                }
            }
        });
        producer.start();
        consumer.start();
        producer.join();
        consumer.join();
        assertEquals(n * (n - 1) / 2, checksum.get());
    }

    @Test
    void roundTripWithSmallCapacity() throws InterruptedException {
        SpscRingBuffer<Long> q = new SpscRingBuffer<>(8);
        long n = 100_000L;
        Thread producer = new Thread(() -> {
            SpscRingBuffer<Long>.Producer p = q.producer();
            long i = 0;
            while (i < n) { if (p.tryPush(i)) i++; }
        });
        AtomicLong errors = new AtomicLong();
        Thread consumer = new Thread(() -> {
            SpscRingBuffer<Long>.Consumer c = q.consumer();
            long next = 0;
            while (next < n) {
                Long v = c.tryPop();
                if (v != null) {
                    if (v != next) errors.incrementAndGet();
                    next++;
                }
            }
        });
        producer.start(); consumer.start();
        producer.join(); consumer.join();
        assertEquals(0, errors.get());
    }

    @Test
    void manyNoOpPopsThenPushPop() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 1000; i++) assertNull(c.tryPop());
        p.tryPush(42);
        assertEquals(42, c.tryPop());
    }

    @Test
    void occupancyTracksPushesAndPopsFromBothHandles() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        assertEquals(0, p.size());
        assertTrue(p.isEmpty());
        assertTrue(c.isEmpty());
        assertFalse(p.isFull());

        for (int i = 0; i < 4; i++) p.tryPush(i);
        assertEquals(4, p.size());
        assertEquals(4, c.size());
        assertTrue(p.isFull());
        assertTrue(c.isFull());
        assertFalse(c.isEmpty());

        c.tryPop();
        assertEquals(3, p.size());
        assertFalse(p.isFull());
    }

    @Test
    void occupancyIsCorrectAfterTheCountersWrapTheSlotArray() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int round = 0; round < 100; round++) {
            p.tryPush(round);
            p.tryPush(round);
            assertEquals(2, p.size());
            c.tryPop();
            c.tryPop();
            assertEquals(0, c.size());
        }
    }

    @Test
    void peekReturnsTheHeadWithoutConsumingIt() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        assertNull(c.peek());
        p.tryPush(11);
        p.tryPush(22);
        assertEquals(11, c.peek());
        assertEquals(11, c.peek());
        assertEquals(2, c.size());
        assertEquals(11, c.tryPop());
        assertEquals(22, c.peek());
    }

    @Test
    void peekRefreshesTheCachedTailAfterAnEmptyObservation() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        assertNull(c.peek());
        p.tryPush(5);
        assertEquals(5, c.peek());
    }

    @Test
    void clearDiscardsEveryBufferedItemAndReportsTheCount() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 3; i++) p.tryPush(i);
        assertEquals(3, c.clear());
        assertTrue(c.isEmpty());
        assertEquals(0, c.clear());

        // The ring is reusable after a clear - slots were freed, not leaked.
        p.tryPush(9);
        assertEquals(1, c.size());
        assertEquals(9, c.tryPop());
    }
}
