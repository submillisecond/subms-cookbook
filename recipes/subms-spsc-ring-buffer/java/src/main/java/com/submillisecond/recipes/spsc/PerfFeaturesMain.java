package com.submillisecond.recipes.spsc;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.spsc.features.MpmcDisruptor;
import com.submillisecond.recipes.spsc.features.MpscFanIn;
import com.submillisecond.recipes.spsc.features.Metrics;
import com.submillisecond.recipes.spsc.features.WaitStrategies;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BlockingSpscConsumer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BlockingSpscProducer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BusySpin;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per (variant, operation) - base_enqueue, base_dequeue,
 * enqueue_bulk, dequeue_bulk, wait_enqueue, wait_dequeue, fanin_enqueue,
 * fanin_dequeue, publish, consume, metrics_enqueue, metrics_dequeue - with the
 * SAME stage names as the Rust bench so the cookbook FeaturePicker columns line
 * up across languages. JSON contract goes to stdout.
 *
 * <p>Everything runs single-threaded: one operation at a time, timed in
 * isolation, so the numbers are per-op latency rather than two-thread
 * throughput. Rings are sized so the base push/pop passes never go full /
 * empty, and the concurrent-shaped features are driven one op at a time
 * against an uncontended counter. Java has no compile-time feature gates, so
 * every variant is always emitted (the Rust bench gates each behind a Cargo
 * feature).
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.spsc.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final int SEED = 0;
    private static final int BULK_BATCH = 32;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("spsc-ring-buffer-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Integer.toString(SEED));
        h.meta("subms.recipe.slug", "subms-spsc-ring-buffer");
        h.meta("subms.recipe.category", "concurrency");

        base(h);
        bulk(h);
        waitStrategies(h);
        mpscFanIn(h);
        mpmcDisruptor(h);
        metrics(h);

        h.writeJson(System.out);
    }

    private static final int WARMUP = 20_000;

    // ---------- base ----------
    // Capacity is rounded up past ENTRIES so every push lands and every pop
    // returns non-null; the timed path is the uncontested fast path on each side.
    // Warmup runs on a throwaway ring: warming the real ring would fill it past
    // capacity (WARMUP + ENTRIES > cap) and flip the push path to the full branch.
    private static void base(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "base");
        warmPushPop();

        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(ENTRIES);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();

        SubMsPerfHarness.Stage enq = h.stage("base_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            final long v = i;
            enq.time(() -> tx.tryPush(v));
        }

        SubMsPerfHarness.Stage deq = h.stage("base_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            deq.time(rx::tryPop);
        }
    }

    // Drive tryPush + tryPop to C2 on a throwaway ring kept balanced so neither
    // path takes its full / empty branch.
    private static void warmPushPop() {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();
        for (int i = 0; i < WARMUP; i++) {
            tx.tryPush((long) i);
            rx.tryPop();
        }
    }

    // ---------- bulk ----------
    // One timed call moves a BULK_BATCH-wide slice, so the per-item atomic
    // load + cache check is amortised behind a single fence. The stage count
    // is the number of batch calls, not the item count.
    private static void bulk(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "bulk");
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(ENTRIES);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();
        Long[] batch = new Long[BULK_BATCH];
        for (int i = 0; i < BULK_BATCH; i++) batch[i] = (long) i;
        int calls = ENTRIES / BULK_BATCH;

        // Warm bulk push + pop on a throwaway ring, balanced per iteration so
        // neither side hits the full / empty short branch.
        warmBulk(batch);

        SubMsPerfHarness.Stage enq = h.stage("enqueue_bulk", calls).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < calls; i++) {
            enq.time(() -> tx.tryPushBulk(batch));
        }

        Long[] out = new Long[BULK_BATCH];
        SubMsPerfHarness.Stage deq = h.stage("dequeue_bulk", calls).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < calls; i++) {
            deq.time(() -> rx.tryPopBulk(out));
        }
    }

    private static void warmBulk(Long[] batch) {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        SpscRingBuffer<Long>.Producer tx = ring.producer();
        SpscRingBuffer<Long>.Consumer rx = ring.consumer();
        Long[] out = new Long[BULK_BATCH];
        int iters = WARMUP / BULK_BATCH;
        for (int i = 0; i < iters; i++) {
            tx.tryPushBulk(batch);
            rx.tryPopBulk(out);
        }
    }

    // ---------- wait-strategies ----------
    // BusySpin wrappers, single-threaded round-trip: push then pop. The slot
    // is always immediately available so the strategy's waitOnce() never
    // fires; this measures the blocking-wrapper overhead on the fast path.
    private static void waitStrategies(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "wait-strategies");
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        BlockingSpscProducer<Long> p =
                new BlockingSpscProducer<>(ring.producer(), new BusySpin());
        BlockingSpscConsumer<Long> c =
                new BlockingSpscConsumer<>(ring.consumer(), new BusySpin());

        // Warm the blocking wrappers on a throwaway pair, round-tripping so the
        // strategy's waitOnce() never fires (matches the timed fast path).
        warmWait();

        SubMsPerfHarness.Stage enq = h.stage("wait_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            final long v = i;
            enq.time(() -> p.push(v));
            // Drain immediately so the next push never blocks on a full ring.
            c.pop();
        }

        // Separate pass to time the consumer fast path in isolation.
        SubMsPerfHarness.Stage deq = h.stage("wait_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            p.push(i);
            deq.time(c::pop);
        }
    }

    private static void warmWait() {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        BlockingSpscProducer<Long> p =
                new BlockingSpscProducer<>(ring.producer(), new BusySpin());
        BlockingSpscConsumer<Long> c =
                new BlockingSpscConsumer<>(ring.consumer(), new BusySpin());
        for (int i = 0; i < WARMUP; i++) {
            p.push((long) i);
            c.pop();
        }
    }

    // ---------- mpsc-fan-in ----------
    // N independent SPSC rings, one consumer round-robining across them.
    // Producers are pushed round-robin so each ring fills evenly; the
    // consumer then drains via its cursor. Timed one op at a time.
    private static void mpscFanIn(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "mpsc-fan-in");
        final int producerCount = 4;
        int perRing = ENTRIES / producerCount;
        MpscFanIn<Long> fanin = new MpscFanIn<>(producerCount, nextPow2(perRing));
        List<MpscFanIn<Long>.Producer> producers = new ArrayList<>(producerCount);
        for (int i = 0; i < producerCount; i++) producers.add(fanin.producer(i));
        MpscFanIn<Long>.Consumer consumer = fanin.consumer();

        // Warm push + round-robin pop on a throwaway fan-in, balanced so rings
        // never fill and the consumer cursor always finds a non-empty ring.
        warmFanIn(producerCount);

        SubMsPerfHarness.Stage enq = h.stage("fanin_enqueue", perRing * producerCount).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < perRing; i++) {
            final long v = i;
            for (MpscFanIn<Long>.Producer pr : producers) {
                enq.time(() -> pr.tryPush(v));
            }
        }

        SubMsPerfHarness.Stage deq = h.stage("fanin_dequeue", perRing * producerCount).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < perRing * producerCount; i++) {
            deq.time(consumer::tryPop);
        }
    }

    private static void warmFanIn(int producerCount) {
        MpscFanIn<Long> fanin = new MpscFanIn<>(producerCount, 1024);
        List<MpscFanIn<Long>.Producer> producers = new ArrayList<>(producerCount);
        for (int i = 0; i < producerCount; i++) producers.add(fanin.producer(i));
        MpscFanIn<Long>.Consumer consumer = fanin.consumer();
        for (int i = 0; i < WARMUP; i++) {
            for (MpscFanIn<Long>.Producer pr : producers) pr.tryPush((long) i);
            for (int j = 0; j < producerCount; j++) consumer.tryPop();
        }
    }

    // ---------- mpmc-disruptor ----------
    // Single producer, single consumer, publish then consume interleaved so
    // the ring never fills (the producer gates behind the slowest consumer).
    // Measures the CAS-claim + publish path and the sequence-barrier consume
    // path one op at a time.
    private static void mpmcDisruptor(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "mpmc-disruptor");
        MpmcDisruptor<Long> ring = new MpmcDisruptor<>(1024, 1);
        MpmcDisruptor<Long>.Producer producer = ring.producer();
        MpmcDisruptor<Long>.Consumer consumer = ring.consumer(0);

        // Warm the CAS-claim + sequence-barrier paths on a throwaway disruptor,
        // interleaved so the producer never gates behind the consumer.
        warmDisruptor();

        // Create both stages up front, then fetch each by name per iteration so
        // publish + consume interleave without filling the ring.
        h.stage("publish", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        h.stage("consume", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            final long v = i;
            h.stage("publish").time(() -> producer.tryPublish(v));
            h.stage("consume").time(consumer::tryConsume);
        }
    }

    private static void warmDisruptor() {
        MpmcDisruptor<Long> ring = new MpmcDisruptor<>(1024, 1);
        MpmcDisruptor<Long>.Producer producer = ring.producer();
        MpmcDisruptor<Long>.Consumer consumer = ring.consumer(0);
        for (int i = 0; i < WARMUP; i++) {
            producer.tryPublish((long) i);
            consumer.tryConsume();
        }
    }

    // ---------- metrics ----------
    // Instrumented wrap; one increment per op on top of the base push/pop.
    private static void metrics(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "metrics");
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(ENTRIES);
        Metrics.Instrumented<Long> inst =
                new Metrics.Instrumented<>(ring.producer(), ring.consumer());
        Metrics.InstrumentedProducer<Long> tx = inst.producer;
        Metrics.InstrumentedConsumer<Long> rx = inst.consumer;

        // Warm the instrumented push / pop on a throwaway wrapper, balanced so
        // the underlying ring stays well clear of full.
        warmMetrics();

        SubMsPerfHarness.Stage enq = h.stage("metrics_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            final long v = i;
            enq.time(() -> tx.tryPush(v));
        }

        SubMsPerfHarness.Stage deq = h.stage("metrics_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            deq.time(rx::tryPop);
        }
    }

    private static void warmMetrics() {
        SpscRingBuffer<Long> ring = new SpscRingBuffer<>(1024);
        Metrics.Instrumented<Long> inst =
                new Metrics.Instrumented<>(ring.producer(), ring.consumer());
        Metrics.InstrumentedProducer<Long> tx = inst.producer;
        Metrics.InstrumentedConsumer<Long> rx = inst.consumer;
        for (int i = 0; i < WARMUP; i++) {
            tx.tryPush((long) i);
            rx.tryPop();
        }
    }

    private static int nextPow2(int n) {
        int x = 1;
        while (x < n) x <<= 1;
        return x;
    }

    private PerfFeaturesMain() {}
}
