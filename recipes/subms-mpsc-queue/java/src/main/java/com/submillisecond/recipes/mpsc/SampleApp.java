package com.submillisecond.recipes.mpsc;

import com.submillisecond.recipes.mpsc.features.BatchMpscQueue;
import com.submillisecond.recipes.mpsc.features.BoundedMpscQueue;
import com.submillisecond.recipes.mpsc.features.JavaAffinity;
import com.submillisecond.recipes.mpsc.features.MetricsMpscQueue;
import com.submillisecond.recipes.mpsc.features.MpmcQueue;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Sample app: a tour of {@code subms-mpsc-queue} in an order-entry setting - N
 * gateway threads funnel orders into one matching-engine consumer. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.mpsc.SampleApp}
 *
 * <ul>
 *   <li>base     - N order-entry gateways fan in to one matching-engine consumer
 *   <li>mpmc     - shard the match loop across several consumers on one ring
 *   <li>bounded  - a fixed-capacity inbox that sheds load rather than the heap
 *   <li>batch    - the match loop drains one tick's orders in a single fenced pass
 *   <li>metrics  - a health snapshot of enqueue / dequeue counts
 *   <li>affinity - the mirror of the Rust pinning API (best-effort on the JVM)
 * </ul>
 */
public final class SampleApp {

    private static final int GATEWAYS = 4;
    private static final int ORDERS_PER_GATEWAY = 1_000;

    private SampleApp() {}

    static long orderId(int gateway, int seq) {
        return ((long) gateway << 32) | seq;
    }

    public static void main(String[] args) throws InterruptedException {
        baseOrderFanIn();
        mpmcShardedMatch();
        boundedInboxBackpressure();
        batchDrainPerTick();
        metricsHealthSnapshot();
        affinityPinMatchLoop();
    }

    /**
     * Base API: every order-entry gateway pushes onto one shared queue and a
     * single matching-engine thread drains it. A {@code null} from
     * {@code tryPoll} is disambiguated by {@code isInconsistent} so a gateway
     * mid-publish is not mistaken for an empty queue.
     */
    static void baseOrderFanIn() throws InterruptedException {
        System.out.println("== base: order-entry gateways fan in to one matching engine ==");
        MpscQueue<Long> q = new MpscQueue<>();

        Thread[] gateways = new Thread[GATEWAYS];
        for (int g = 0; g < GATEWAYS; g++) {
            final int gid = g;
            gateways[g] = new Thread(() -> {
                for (int seq = 0; seq < ORDERS_PER_GATEWAY; seq++) {
                    q.push(orderId(gid, seq));
                }
            });
            gateways[g].start();
        }
        for (Thread t : gateways) t.join();

        int total = GATEWAYS * ORDERS_PER_GATEWAY;
        System.out.println("  inbox depth before the match loop starts: " + q.size());
        long first = q.peek();
        System.out.println("  head of book: gateway " + (first >> 32)
            + " seq " + (first & 0xffff_ffffL));

        int[] perGateway = new int[GATEWAYS];
        long[] lastSeq = new long[GATEWAYS];
        java.util.Arrays.fill(lastSeq, -1L);
        int matched = 0;
        while (matched < total) {
            Long order = q.tryPoll();
            if (order != null) {
                int g = (int) (order >> 32);
                long seq = order & 0xffff_ffffL;
                if (seq <= lastSeq[g]) {
                    throw new AssertionError("orders from one gateway must stay in FIFO order");
                }
                lastSeq[g] = seq;
                perGateway[g]++;
                matched++;
            } else if (!q.isInconsistent()) {
                break;
            }
        }

        System.out.println("  " + GATEWAYS + " gateways x " + ORDERS_PER_GATEWAY
            + " orders -> matched " + matched);
        System.out.println("  per-gateway tally: " + java.util.Arrays.toString(perGateway));
        if (matched != total) throw new AssertionError("no order dropped, none duplicated");
        for (int count : perGateway) {
            if (count != ORDERS_PER_GATEWAY) throw new AssertionError("every gateway fully drained");
        }

        // Kill switch: a venue disconnect voids everything still queued rather
        // than matching it against a book that has moved on.
        for (int seq = 0; seq < 32; seq++) q.push(orderId(0, seq));
        int voided = q.clear();
        System.out.println("  kill switch voided " + voided
            + " queued orders, inbox empty: " + q.isEmpty());
        if (voided != 32 || !q.isEmpty()) throw new AssertionError("the kill switch drains the inbox");
    }

    /**
     * mpmc: when one match loop cannot keep up, {@link MpmcQueue} is a bounded
     * Disruptor-style ring where several consumer shards race the head; the CAS
     * loser refreshes and retries. Producers race the tail the same way.
     */
    static void mpmcShardedMatch() throws InterruptedException {
        System.out.println("\n== mpmc: shard the match loop across several consumers ==");
        int shards = 3;
        int total = GATEWAYS * ORDERS_PER_GATEWAY;
        MpmcQueue<Long> ring = new MpmcQueue<>(1_024);

        Thread[] gateways = new Thread[GATEWAYS];
        for (int g = 0; g < GATEWAYS; g++) {
            final int gid = g;
            gateways[g] = new Thread(() -> {
                for (int seq = 0; seq < ORDERS_PER_GATEWAY; seq++) {
                    while (!ring.tryEnqueue(orderId(gid, seq))) {
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
        // casRetries() is the contention read-out, deliberately not printed: it
        // is a property of how the OS scheduled these threads on this run.
        System.out.println("  " + shards + " shards drained " + matched.get()
            + " orders, ring empty: " + ring.isEmpty());
        if (matched.get() != total) {
            throw new AssertionError("shards together drain every order exactly once");
        }
        if (ring.currentProducerIndex() != ring.currentConsumerIndex()) {
            throw new AssertionError("every claimed slot was consumed");
        }
    }

    /**
     * bounded: a fixed-capacity ring gives the gateway backpressure. The base
     * queue lets a slow match loop grow the heap without bound; the bounded
     * inbox returns {@code false} on full so the gateway can retry or shed load.
     */
    static void boundedInboxBackpressure() {
        System.out.println("\n== bounded: a fixed-capacity inbox that pushes back ==");
        BoundedMpscQueue<Long> inbox = new BoundedMpscQueue<>(4);
        int cap = inbox.capacity();

        int accepted = 0;
        int rejected = 0;
        for (int seq = 0; seq < cap + 2; seq++) {
            if (inbox.tryEnqueue(orderId(0, seq))) accepted++;
            else rejected++;
        }
        System.out.println("  capacity " + cap + ": accepted " + accepted
            + ", shed " + rejected + " while full");
        if (accepted != cap) throw new AssertionError("accepts exactly one full ring");
        if (rejected != 2) throw new AssertionError("the overflow is handed back, not queued");

        if (!inbox.isFull()) throw new AssertionError("a full ring reads as full");
        if (inbox.tryDequeue() == null) throw new AssertionError("match loop consumes one order");
        if (!inbox.tryEnqueue(orderId(0, 99))) {
            throw new AssertionError("a freed slot reopens the inbox");
        }

        // The two monotonic cursors are what a health check scrapes: their
        // difference is inbox lag, and each on its own gives a rate between polls.
        System.out.println("  producer index " + inbox.currentProducerIndex()
            + " - consumer index " + inbox.currentConsumerIndex()
            + " = lag " + inbox.size());
        if (inbox.currentProducerIndex() - inbox.currentConsumerIndex() != inbox.size()) {
            throw new AssertionError("lag is the gap between the two cursors");
        }
    }

    /**
     * batch: a gateway publishes a decoded run with one head swap, and a match
     * loop on a tick drains a whole tick's worth of orders in one fenced pass.
     * Both directions pay one atomic per call instead of one per order.
     */
    static void batchDrainPerTick() {
        System.out.println("\n== batch: publish and drain a whole tick in one pass ==");
        final int tick = 256;
        final int burst = 50;
        int total = 1_000;
        BatchMpscQueue<Long> q = new BatchMpscQueue<>();

        // A gateway that decodes a wire frame already holds a run of orders.
        // One head swap publishes the whole run instead of burst of them.
        Long[] run = new Long[burst];
        int published = 0;
        while (published < total) {
            for (int i = 0; i < burst; i++) run[i] = orderId(0, published + i);
            published += q.pushBatch(run);
        }
        System.out.println("  " + published + " orders published in "
            + (total / burst) + " swaps");
        if (published != total) throw new AssertionError("every burst is published");

        Long[] buf = new Long[tick];
        int ticks = 0;
        int matched = 0;
        while (true) {
            int n = q.tryDequeueBatch(buf);
            if (n == 0) break;
            ticks++;
            matched += n;
        }
        System.out.println("  drained " + matched + " orders across " + ticks
            + " ticks of up to " + tick);
        if (matched != total) throw new AssertionError("every queued order is drained");
        int expectedTicks = (total + tick - 1) / tick;
        if (ticks != expectedTicks) throw new AssertionError("each tick drains a full buffer");

        // The callback form skips the buffer entirely when the match loop's
        // work is per-order anyway. Here it accumulates notional.
        Long[] tail = new Long[64];
        for (int seq = 0; seq < 64; seq++) tail[seq] = orderId(1, seq);
        q.pushBatch(tail);
        AtomicLong notional = new AtomicLong();
        int handled = q.drain(order -> notional.addAndGet(order & 0xffff_ffffL), tick);
        System.out.println("  drain callback handled " + handled
            + " orders, notional " + notional.get());
        if (handled != 64 || notional.get() != 64L * 63 / 2) {
            throw new AssertionError("the callback form needs no buffer");
        }
        if (!q.isEmpty()) throw new AssertionError("the queue is drained");
    }

    /**
     * metrics: wrap the queue in per-instance counters to answer "is this inbox
     * actually contended" from a health snapshot rather than a guess.
     */
    static void metricsHealthSnapshot() {
        System.out.println("\n== metrics: a health snapshot of the inbox ==");
        MetricsMpscQueue<Long> q = new MetricsMpscQueue<>();
        for (int seq = 0; seq < 500; seq++) q.push(orderId(0, seq));
        int matched = 0;
        while (matched < 500) {
            if (q.tryPoll() != null) matched++;
        }
        q.tryPoll(); // a miss on the drained queue registers a dequeue failure

        MetricsMpscQueue.Snapshot snap = q.snapshot();
        System.out.println("  enqueueOk=" + snap.enqueueOk + " dequeueOk=" + snap.dequeueOk
            + " dequeueFail=" + snap.dequeueFail);
        if (snap.enqueueOk != 500) throw new AssertionError("every push is counted");
        if (snap.dequeueOk != 500) throw new AssertionError("every matched order is counted");
        if (snap.dequeueFail < 1) throw new AssertionError("the miss on the drained queue is counted");
    }

    /**
     * affinity: the JVM has no portable pinning API, so this mirrors the Rust
     * surface and reports {@code UNSUPPORTED} on the stock JDK. An empty core
     * set is always rejected.
     */
    static void affinityPinMatchLoop() {
        System.out.println("\n== affinity: the pinning API mirror ==");
        JavaAffinity.AffinityStatus status = JavaAffinity.setAffinity(0);
        System.out.println("  setAffinity(0) -> " + status
            + " (supported: " + JavaAffinity.isSupported() + ")");
        if (JavaAffinity.setAffinity() != JavaAffinity.AffinityStatus.INVALID_CORE) {
            throw new AssertionError("an empty core set is always rejected");
        }
    }
}
