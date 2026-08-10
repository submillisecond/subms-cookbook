package com.submillisecond.recipes.mpsc;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.submillisecond.recipes.mpsc.features.BatchMpscQueue;
import com.submillisecond.recipes.mpsc.features.BoundedMpscQueue;
import com.submillisecond.recipes.mpsc.features.JavaAffinity;
import com.submillisecond.recipes.mpsc.features.MetricsMpscQueue;
import com.submillisecond.recipes.mpsc.features.MpmcQueue;
import java.util.Arrays;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.Test;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    private static final int GATEWAYS = 4;
    private static final int ORDERS_PER_GATEWAY = 1_000;

    @Test
    void quickstart() {
        // quickstart:begin
        MpscQueue<Integer> q = new MpscQueue<>();
        q.push(7);
        q.push(8);
        assertEquals(7, (int) q.tryPoll()); // single consumer drains in FIFO order
        assertEquals(8, (int) q.tryPoll());
        assertNull(q.tryPoll()); // drained
        // quickstart:end
    }

    @Test
    void orderFanInLosesNothingAndKeepsGatewayOrder() throws InterruptedException {
        MpscQueue<Long> q = new MpscQueue<>();
        Thread[] gateways = new Thread[GATEWAYS];
        for (int g = 0; g < GATEWAYS; g++) {
            final int gid = g;
            gateways[g] = new Thread(() -> {
                for (int seq = 0; seq < ORDERS_PER_GATEWAY; seq++) {
                    q.push(SampleApp.orderId(gid, seq));
                }
            });
            gateways[g].start();
        }
        for (Thread t : gateways) t.join();

        int total = GATEWAYS * ORDERS_PER_GATEWAY;
        assertEquals(total, q.size(), "the whole backlog is visible before the drain");
        assertFalse(q.isEmpty());
        assertNotNull(q.peek(), "the head is readable without consuming it");

        int[] perGateway = new int[GATEWAYS];
        long[] lastSeq = new long[GATEWAYS];
        Arrays.fill(lastSeq, -1L);
        int matched = 0;
        while (matched < total) {
            Long order = q.tryPoll();
            if (order != null) {
                int g = (int) (order >> 32);
                long seq = order & 0xffff_ffffL;
                assertTrue(seq > lastSeq[g], "per-gateway FIFO preserved");
                lastSeq[g] = seq;
                perGateway[g]++;
                matched++;
            } else if (!q.isInconsistent()) {
                break;
            }
        }

        assertEquals(total, matched, "every order matched exactly once");
        for (int count : perGateway) {
            assertEquals(ORDERS_PER_GATEWAY, count, "every gateway fully drained");
        }

        for (int seq = 0; seq < 32; seq++) q.push(SampleApp.orderId(0, seq));
        assertEquals(32, q.clear(), "the kill switch voids the queued orders");
        assertTrue(q.isEmpty());
    }

    @Test
    void mpmcShardsDrainEveryOrderOnce() throws InterruptedException {
        int shards = 3;
        int perGateway = 2_000;
        int total = GATEWAYS * perGateway;
        MpmcQueue<Long> ring = new MpmcQueue<>(1_024);

        Thread[] gateways = new Thread[GATEWAYS];
        for (int g = 0; g < GATEWAYS; g++) {
            final int gid = g;
            gateways[g] = new Thread(() -> {
                for (int seq = 0; seq < perGateway; seq++) {
                    while (!ring.tryEnqueue(SampleApp.orderId(gid, seq))) {
                        Thread.onSpinWait();
                    }
                }
            });
            gateways[g].start();
        }

        AtomicInteger matched = new AtomicInteger(0);
        Thread[] consumers = new Thread[shards];
        for (int s = 0; s < shards; s++) {
            consumers[s] = new Thread(() -> {
                while (true) {
                    if (ring.tryDequeue() != null) {
                        matched.incrementAndGet();
                    } else if (matched.get() >= total) {
                        break;
                    } else {
                        Thread.onSpinWait();
                    }
                }
            });
            consumers[s].start();
        }

        for (Thread t : gateways) t.join();
        for (Thread t : consumers) t.join();
        assertEquals(total, matched.get(), "shards together drain every order exactly once");
        assertTrue(ring.isEmpty());
        assertEquals(ring.currentProducerIndex(), ring.currentConsumerIndex());
    }

    @Test
    void boundedInboxShedsWhenFullThenReopens() {
        BoundedMpscQueue<Long> inbox = new BoundedMpscQueue<>(4);
        int cap = inbox.capacity();

        int accepted = 0;
        int rejected = 0;
        for (int seq = 0; seq < cap + 2; seq++) {
            if (inbox.tryEnqueue(SampleApp.orderId(0, seq))) accepted++;
            else rejected++;
        }
        assertEquals(cap, accepted, "accepts exactly one full ring");
        assertEquals(2, rejected, "overflow is handed back");

        assertTrue(inbox.isFull());
        assertTrue(inbox.tryDequeue() != null);
        assertFalse(inbox.isFull());
        assertTrue(inbox.tryEnqueue(SampleApp.orderId(0, 99)), "a freed slot reopens the inbox");
        assertEquals(inbox.size(), inbox.currentProducerIndex() - inbox.currentConsumerIndex(),
            "the two cursors give inbox lag");
    }

    @Test
    void batchDrainsFullTicksUntilTheTail() {
        final int tick = 256;
        final int burst = 50;
        int total = 1_000;
        BatchMpscQueue<Long> q = new BatchMpscQueue<>();
        Long[] run = new Long[burst];
        int published = 0;
        while (published < total) {
            for (int i = 0; i < burst; i++) run[i] = SampleApp.orderId(0, published + i);
            published += q.pushBatch(run);
        }
        assertEquals(total, published, "each burst publishes in one head swap");

        Long[] buf = new Long[tick];
        int ticks = 0;
        int matched = 0;
        while (true) {
            int n = q.tryDequeueBatch(buf);
            if (n == 0) break;
            ticks++;
            matched += n;
        }
        assertEquals(total, matched, "every queued order is drained");
        assertEquals((total + tick - 1) / tick, ticks);

        Long[] tail = new Long[64];
        for (int seq = 0; seq < 64; seq++) tail[seq] = SampleApp.orderId(1, seq);
        q.pushBatch(tail);
        AtomicLong notional = new AtomicLong();
        int handled = q.drain(order -> notional.addAndGet(order & 0xffff_ffffL), tick);
        assertEquals(64, handled, "the callback form needs no buffer");
        assertEquals(64L * 63 / 2, notional.get());
        assertTrue(q.isEmpty());
    }

    @Test
    void metricsSnapshotCountsPushesAndPops() {
        MetricsMpscQueue<Long> q = new MetricsMpscQueue<>();
        for (int seq = 0; seq < 500; seq++) q.push(SampleApp.orderId(0, seq));
        int matched = 0;
        while (matched < 500) {
            if (q.tryPoll() != null) matched++;
        }
        q.tryPoll();

        MetricsMpscQueue.Snapshot snap = q.snapshot();
        assertEquals(500, snap.enqueueOk);
        assertEquals(500, snap.dequeueOk);
        assertTrue(snap.dequeueFail >= 1);
    }

    @Test
    void affinityMirrorsRustSurface() {
        assertEquals(JavaAffinity.AffinityStatus.UNSUPPORTED, JavaAffinity.setAffinity(0));
        assertEquals(JavaAffinity.AffinityStatus.INVALID_CORE, JavaAffinity.setAffinity());
        assertThrows(NullPointerException.class, () -> JavaAffinity.setAffinity((int[]) null));
    }
}
