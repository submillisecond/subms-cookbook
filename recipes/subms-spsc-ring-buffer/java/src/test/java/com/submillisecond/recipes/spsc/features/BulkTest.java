package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;
import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class BulkTest {

    @Test
    void bulkEnqueueThenBulkDequeueRoundTrip() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(16);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        Integer[] src = new Integer[]{0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
        assertEquals(10, Bulk.tryEnqueueBulk(p, src));
        Integer[] out = new Integer[10];
        assertEquals(10, Bulk.tryDequeueBulk(c, out));
        assertArrayEquals(src, out);
    }

    @Test
    void bulkEnqueuePartialWhenRingAlmostFull() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        for (int i = 0; i < 3; i++) p.tryPush(i);
        Integer[] more = new Integer[]{100, 101, 102, 103, 104};
        assertEquals(1, Bulk.tryEnqueueBulk(p, more));
    }

    @Test
    void bulkDequeuePartialWhenRingAlmostEmpty() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(8);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 3; i++) p.tryPush(i);
        Integer[] out = new Integer[10];
        assertEquals(3, Bulk.tryDequeueBulk(c, out));
        assertEquals(0, out[0]);
        assertEquals(1, out[1]);
        assertEquals(2, out[2]);
    }

    @Test
    void bulkReturnsZeroOnFull() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        for (int i = 0; i < 4; i++) p.tryPush(i);
        Integer[] more = new Integer[]{99, 100};
        assertEquals(0, p.tryPushBulk(more));
    }

    @Test
    void bulkReturnsZeroOnEmpty() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        Integer[] out = new Integer[4];
        assertEquals(0, c.tryPopBulk(out));
    }

    @Test
    void bulkHandlesZeroLength() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        assertEquals(0, p.tryPushBulk(new Integer[0]));
        assertEquals(0, c.tryPopBulk(new Integer[0]));
        assertEquals(0, p.tryPushBulk(null));
        assertEquals(0, c.tryPopBulk(null));
    }

    @Test
    void bulkRejectsNulls() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(4);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        Integer[] bad = new Integer[]{1, null, 3};
        assertThrows(NullPointerException.class, () -> p.tryPushBulk(bad));
    }

    @Test
    void bulkWrapsAroundCorrectly() {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(8);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();
        for (int i = 0; i < 6; i++) p.tryPush(i);
        for (int i = 0; i < 6; i++) c.tryPop();
        Integer[] src = new Integer[]{10, 11, 12, 13, 14, 15};
        assertEquals(6, p.tryPushBulk(src));
        Integer[] out = new Integer[6];
        assertEquals(6, c.tryPopBulk(out));
        assertArrayEquals(src, out);
    }

    @Test
    void bulkRoundTripUnderTwoThreads() throws InterruptedException {
        SpscRingBuffer<Long> q = new SpscRingBuffer<>(1024);
        long n = 200_000L;
        AtomicLong errs = new AtomicLong();

        Thread producer = new Thread(() -> {
            SpscRingBuffer<Long>.Producer p = q.producer();
            long sent = 0;
            while (sent < n) {
                int batch = (int) Math.min(32L, n - sent);
                Long[] chunk = new Long[batch];
                for (int i = 0; i < batch; i++) chunk[i] = sent + i;
                int pushed = p.tryPushBulk(chunk);
                sent += pushed;
            }
        });
        Thread consumer = new Thread(() -> {
            SpscRingBuffer<Long>.Consumer c = q.consumer();
            long next = 0;
            Long[] out = new Long[32];
            while (next < n) {
                int got = c.tryPopBulk(out);
                for (int i = 0; i < got; i++) {
                    if (out[i] != next) errs.incrementAndGet();
                    next++;
                }
            }
        });
        producer.start();
        consumer.start();
        producer.join();
        consumer.join();
        assertEquals(0, errs.get());
    }
}
