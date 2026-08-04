package com.submillisecond.recipes.spsc;

import com.submillisecond.recipes.spsc.features.Bulk;
import com.submillisecond.recipes.spsc.features.Metrics;
import com.submillisecond.recipes.spsc.features.MpmcDisruptor;
import com.submillisecond.recipes.spsc.features.MpscFanIn;
import com.submillisecond.recipes.spsc.features.WaitStrategies;

import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Sample app: a tour of {@code subms-spsc-ring-buffer}, base API first, then each
 * optional feature. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.spsc.SampleApp}
 *
 * <p>Domain: a market-data feed handler thread hands ticks (element = the tick's
 * sequence number) to a strategy thread over the wait-free ring. A full ring
 * makes the feed handler drop the tick (backpressure) rather than block.
 *
 * <ul>
 *   <li>base            - feed-handler -&gt; strategy handoff, drop-on-full
 *   <li>bulk            - drain a NIC receive batch in one fenced call
 *   <li>wait-strategies - a blocking handoff when a wakeup cost is affordable
 *   <li>mpsc-fan-in     - many venue feeds into one strategy, still wait-free per feed
 *   <li>mpmc-disruptor  - broadcast one tick stream to strategy + risk monitor
 *   <li>metrics         - per-instance enqueue/dequeue + max-depth counters
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws InterruptedException {
        baseFeedToStrategy();
        bulkBatchIngest();
        blockingHandoff();
        manyVenueFanIn();
        broadcastToStrategyAndRisk();
        instrumentedHandoff();
    }

    /** Base API: a feed-handler thread pushes ticks to a strategy thread. Both
     * ends are wait-free; a full ring makes {@code tryPush} return false so the
     * feed handler drops the tick instead of stalling the hot path. */
    static void baseFeedToStrategy() throws InterruptedException {
        System.out.println("== base: feed-handler -> strategy handoff ==");

        // Drop-on-full is the caller's decision, shown on a small ring:
        // capacity 4, six ticks offered, the last two are dropped.
        SpscRingBuffer<Long> small = new SpscRingBuffer<>(4);
        SpscRingBuffer<Long>.Producer stx = small.producer();
        int dropped = 0;
        for (long seq = 0; seq < 6; seq++) {
            if (!stx.tryPush(seq)) dropped++;
        }
        System.out.println("  cap-4 ring, 6 offered -> " + dropped + " dropped under backpressure");
        if (dropped != 2) throw new AssertionError("two ticks past capacity are dropped, not blocked");

        // Occupancy is what a queue-depth alarm reads, and peek lets the strategy
        // inspect the oldest tick before deciding to consume it.
        SpscRingBuffer<Long>.Consumer srx = small.consumer();
        System.out.println("  depth " + srx.size() + "/" + small.capacity()
            + " full=" + stx.isFull() + ", oldest queued seq " + srx.peek());
        System.out.println("  dropped " + srx.clear() + " stale ticks on resync");

        // Steady state: a drained ring loses nothing and preserves feed order.
        long n = 50_000;
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();

        Thread feed = new Thread(() -> {
            for (long seq = 0; seq < n; seq++) {
                while (!tx.tryPush(seq)) Thread.onSpinWait();
            }
        });
        AtomicLong received = new AtomicLong();
        Thread strategy = new Thread(() -> {
            long expected = 0;
            while (expected < n) {
                Long tick = rx.tryPop();
                if (tick != null) {
                    if (tick != expected) throw new AssertionError("ticks arrive in feed order");
                    expected++;
                }
            }
            received.set(expected);
        });
        feed.start();
        strategy.start();
        feed.join();
        strategy.join();
        System.out.println("  streamed " + received.get() + " ticks in order, zero loss when drained");
        if (received.get() != n) throw new AssertionError("every tick received");
    }

    /** bulk: a feed handler lifts a whole batch off one NIC receive.
     * {@code tryEnqueueBulk} copies the run in behind a single release fence
     * instead of one per tick; the strategy drains behind one acquire fence. */
    static void bulkBatchIngest() {
        System.out.println("\n== bulk: batch a NIC receive into the ring ==");
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(16);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();

        Long[] batch = new Long[10];
        for (int i = 0; i < batch.length; i++) batch[i] = 20_000L + i;

        int pushed = Bulk.tryEnqueueBulk(tx, batch);
        System.out.println("  offered " + batch.length + " ticks, took " + pushed + " in one fenced call");
        if (pushed != 10) throw new AssertionError("full batch fits");

        Long[] out = new Long[10];
        int drained = Bulk.tryDequeueBulk(rx, out);
        System.out.println("  drained " + drained + " in one fenced call");
        if (drained != 10) throw new AssertionError("full batch drains");
        for (int i = 0; i < out.length; i++) {
            if (!out[i].equals(batch[i])) throw new AssertionError("bulk preserves order");
        }
    }

    /** wait-strategies: wrap the non-blocking ends in a blocking handle with a
     * backoff policy. {@code YieldStrategy} lets other threads run between
     * retries, a sensible default when feed and strategy share cores. */
    static void blockingHandoff() throws InterruptedException {
        System.out.println("\n== wait-strategies: blocking handoff (yield backoff) ==");
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(8);
        WaitStrategies.BlockingSpscProducer<Long> producer =
                new WaitStrategies.BlockingSpscProducer<>(ring.producer(), new WaitStrategies.YieldStrategy());
        WaitStrategies.BlockingSpscConsumer<Long> consumer =
                new WaitStrategies.BlockingSpscConsumer<>(ring.consumer(), new WaitStrategies.YieldStrategy());

        long n = 5_000;
        Thread feed = new Thread(() -> {
            for (long seq = 0; seq < n; seq++) producer.push(seq);
        });
        Thread strategy = new Thread(() -> {
            for (long expected = 0; expected < n; expected++) {
                long tick = consumer.pop();
                if (tick != expected) throw new AssertionError("blocking pop keeps feed order");
            }
        });
        feed.start();
        strategy.start();
        feed.join();
        strategy.join();
        System.out.println("  handed off " + n + " ticks, producer blocked on full instead of dropping");
    }

    /** mpsc-fan-in: several venue feeds, one strategy consumer. Each feed owns
     * an independent SPSC ring (still wait-free against its own counter); the
     * consumer round-robins so a quiet venue never starves a busy one. */
    static void manyVenueFanIn() throws InterruptedException {
        System.out.println("\n== mpsc-fan-in: three venue feeds -> one strategy ==");
        int venues = 3;
        long perVenue = 20_000;
        MpscFanIn<Long> fanIn = new MpscFanIn<>(venues, 256);
        MpscFanIn<Long>.Consumer consumer = fanIn.consumer();

        Thread[] feeds = new Thread[venues];
        for (int v = 0; v < venues; v++) {
            MpscFanIn<Long>.Producer p = fanIn.producer(v);
            feeds[v] = new Thread(() -> {
                for (long seq = 0; seq < perVenue; seq++) {
                    while (!p.tryPush(seq)) Thread.onSpinWait();
                }
            });
        }
        long total = perVenue * venues;
        AtomicLong got = new AtomicLong();
        Thread strategy = new Thread(() -> {
            long local = 0;
            while (local < total) {
                if (consumer.tryPop() != null) local++;
            }
            got.set(local);
        });
        for (Thread f : feeds) f.start();
        strategy.start();
        for (Thread f : feeds) f.join();
        strategy.join();
        System.out.println("  " + venues + " feeds x " + perVenue + " ticks -> consumer drained " + got.get());
        if (got.get() != total) throw new AssertionError("every venue drained");
    }

    /** mpmc-disruptor: broadcast one tick stream to independent readers, a
     * strategy and a risk monitor, each of which sees every published tick.
     * (For each tick handled by exactly one reader, use mpsc-fan-in.) */
    static void broadcastToStrategyAndRisk() {
        System.out.println("\n== mpmc-disruptor: broadcast to strategy + risk ==");
        long n = 8;
        MpmcDisruptor<Long> disruptor = new MpmcDisruptor<>(16, 2);
        MpmcDisruptor<Long>.Producer producer = disruptor.producer();
        List<MpmcDisruptor<Long>.Consumer> consumers = disruptor.consumers();
        MpmcDisruptor<Long>.Consumer strategy = consumers.get(0);
        MpmcDisruptor<Long>.Consumer risk = consumers.get(1);

        long published = 0;
        long stratSeen = 0;
        long riskSeen = 0;
        while (published < n) {
            while (published < n && producer.tryPublish(published)) published++;
            Long v;
            while ((v = strategy.tryConsume()) != null) {
                if (v != stratSeen) throw new AssertionError("strategy sees every tick in order");
                stratSeen++;
            }
            while ((v = risk.tryConsume()) != null) {
                if (v != riskSeen) throw new AssertionError("risk sees every tick in order");
                riskSeen++;
            }
        }
        System.out.println("  published " + published + "; strategy saw " + stratSeen + ", risk saw " + riskSeen);
        if (stratSeen != n || riskSeen != n) throw new AssertionError("both readers see every tick");
    }

    /** metrics: wrap a base pair to count enqueue/dequeue success + fail and
     * track the high-water depth, at the cost of one atomic increment per op. */
    static void instrumentedHandoff() {
        System.out.println("\n== metrics: instrumented feed handoff ==");
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(4);
        Metrics.Instrumented<Long> inst = new Metrics.Instrumented<>(ring.producer(), ring.consumer());

        for (long seq = 0; seq < 4; seq++) {
            if (!inst.producer.tryPush(seq)) throw new AssertionError("first 4 fit");
        }
        if (inst.producer.tryPush(99L)) throw new AssertionError("5th is dropped");
        for (int i = 0; i < 4; i++) {
            if (inst.consumer.tryPop() == null) throw new AssertionError("4 drain");
        }
        if (inst.consumer.tryPop() != null) throw new AssertionError("ring empty");

        Metrics.RingMetricsSnapshot snap = inst.metrics.snapshot();
        System.out.println("  enqueued " + snap.enqueueSuccess + " (dropped " + snap.enqueueFail
                + "), dequeued " + snap.dequeueSuccess + " (empty " + snap.dequeueFail
                + "), peak depth " + snap.maxDepthObserved);
        if (snap.enqueueSuccess != 4 || snap.enqueueFail != 1) throw new AssertionError("enqueue counts");
        if (snap.dequeueSuccess != 4 || snap.dequeueFail != 1) throw new AssertionError("dequeue counts");
        if (snap.maxDepthObserved != 4) throw new AssertionError("peak depth");
    }

    private SampleApp() {}
}
