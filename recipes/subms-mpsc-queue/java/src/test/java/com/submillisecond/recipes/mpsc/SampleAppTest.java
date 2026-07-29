package com.submillisecond.recipes.mpsc;

import static org.junit.jupiter.api.Assertions.assertEquals;
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

        assertTrue(inbox.tryDequeue() != null);
        assertTrue(inbox.tryEnqueue(SampleApp.orderId(0, 99)), "a freed slot reopens the inbox");
    }

    @Test
    void batchDrainsFullTicksUntilTheTail() {
        final int tick = 256;
        int total = 1_000;
        BatchMpscQueue<Long> q = new BatchMpscQueue<>();
        for (int seq = 0; seq < total; seq++) q.push(SampleApp.orderId(0, seq));

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
