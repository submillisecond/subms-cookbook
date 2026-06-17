package com.submillisecond.recipes.spsc.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class MpscFanInTest {

    @Test
    void singleProducerActsLikeSpsc() {
        MpscFanIn<Integer> fan = new MpscFanIn<>(1, 4);
        fan.producer(0).tryPush(7);
        assertEquals(7, fan.consumer().tryPop());
        assertNull(fan.consumer().tryPop());
    }

    @Test
    void roundRobinVisitsEveryProducer() {
        MpscFanIn<Integer> fan = new MpscFanIn<>(3, 4);
        fan.producer(0).tryPush(10);
        fan.producer(1).tryPush(20);
        fan.producer(2).tryPush(30);
        List<Integer> got = new ArrayList<>();
        for (int i = 0; i < 3; i++) got.add(fan.consumer().tryPop());
        Collections.sort(got);
        assertEquals(List.of(10, 20, 30), got);
    }

    @Test
    void quietProducerDoesntBlockBusy() {
        MpscFanIn<Integer> fan = new MpscFanIn<>(2, 16);
        for (int i = 0; i < 10; i++) fan.producer(1).tryPush(i);
        List<Integer> got = new ArrayList<>();
        for (int i = 0; i < 10; i++) {
            Integer v = fan.consumer().tryPop();
            assertNotNull(v);
            got.add(v);
        }
        assertEquals(List.of(0, 1, 2, 3, 4, 5, 6, 7, 8, 9), got);
    }

    @Test
    void tryPopOnAllEmptyReturnsNull() {
        MpscFanIn<Integer> fan = new MpscFanIn<>(4, 4);
        assertNull(fan.consumer().tryPop());
    }

    @Test
    void invalidProducerCountThrows() {
        assertThrows(IllegalArgumentException.class, () -> new MpscFanIn<Integer>(0, 4));
    }

    @Test
    void threeProducersOneConsumerUnderThreads() throws InterruptedException {
        MpscFanIn<Long> fan = new MpscFanIn<>(3, 256);
        long perProducer = 50_000L;
        AtomicLong consumed = new AtomicLong();
        long total = perProducer * 3;

        Thread consumer = new Thread(() -> {
            long local = 0;
            while (local < total) {
                if (fan.consumer().tryPop() != null) {
                    local++;
                    consumed.incrementAndGet();
                }
            }
        });
        consumer.start();

        Thread[] producers = new Thread[3];
        for (int i = 0; i < 3; i++) {
            final long shard = i;
            final MpscFanIn<Long>.Producer p = fan.producer(i);
            producers[i] = new Thread(() -> {
                for (long j = 0; j < perProducer; j++) {
                    long v = shard * 1_000_000L + j;
                    while (!p.tryPush(v)) Thread.onSpinWait();
                }
            });
            producers[i].start();
        }
        for (Thread t : producers) t.join();
        consumer.join();
        assertEquals(total, consumed.get());
    }

    @Test
    void cursorAdvancesPastDrainedRing() {
        MpscFanIn<Integer> fan = new MpscFanIn<>(2, 4);
        fan.producer(0).tryPush(1);
        assertEquals(1, fan.consumer().tryPop()); // cursor -> 1
        fan.producer(1).tryPush(2);
        assertEquals(2, fan.consumer().tryPop());
    }
}
