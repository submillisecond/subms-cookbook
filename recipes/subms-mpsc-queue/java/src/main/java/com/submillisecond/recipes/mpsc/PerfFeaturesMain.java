package com.submillisecond.recipes.mpsc;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.mpsc.features.BatchMpscQueue;
import com.submillisecond.recipes.mpsc.features.BoundedMpscQueue;
import com.submillisecond.recipes.mpsc.features.JavaAffinity;
import com.submillisecond.recipes.mpsc.features.MetricsMpscQueue;
import com.submillisecond.recipes.mpsc.features.MpmcQueue;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per (variant, operation) - base_enqueue, base_dequeue,
 * mpmc_enqueue, mpmc_dequeue, bounded_enqueue, bounded_dequeue, batch_drain,
 * metrics_enqueue, metrics_dequeue, affinity_set - with the SAME stage names
 * as the Rust bench so the cookbook FeaturePicker columns line up across
 * languages. JSON contract goes to stdout.
 *
 * <p>Concurrent variants ({@code mpmc}) are driven single-threaded here: the
 * point is the per-op latency of the enqueue / dequeue path, not the
 * contention curve. The {@code affinity} variant has no per-op hot-path cost
 * (it pins a thread once at setup), so it is timed once as {@code affinity_set}
 * rather than per-op. Java has no compile-time feature gates, so every variant
 * is always emitted (the Rust bench gates each behind a Cargo feature).
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.mpsc.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final int SEED = 0;
    private static final int BATCH = 256;
    private static final int WARMUP = 20_000;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("mpsc-queue-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Integer.toString(SEED));
        h.meta("subms.recipe.slug", "subms-mpsc-queue");
        h.meta("subms.recipe.category", "concurrency");

        base(h);
        mpmc(h);
        bounded(h);
        batch(h);
        metrics(h);
        affinity(h);

        h.writeJson(System.out);
    }

    // ---------- base ----------
    // Warmup pushes / polls on a throwaway queue: the measured enqueue pass owns
    // a fresh empty queue and the measured dequeue pass drains exactly what it
    // enqueued, so warming in place would shift both balances.
    private static void base(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "base");
        MpscQueue<Long> warm = new MpscQueue<>();
        for (int i = 0; i < WARMUP; i++) warm.push((long) i);
        for (int i = 0; i < WARMUP; i++) warm.tryPoll();

        MpscQueue<Long> q = new MpscQueue<>();
        SubMsPerfHarness.Stage enq = h.stage("base_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            final long v = i;
            enq.time(() -> q.push(v));
        }
        SubMsPerfHarness.Stage deq = h.stage("base_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            deq.time(q::tryPoll);
        }
    }

    // ---------- mpmc ----------
    // Bounded ring: warm on a throwaway, interleaved so neither side hits the
    // full / empty branch. Warming the real ring would fill it (WARMUP + ENTRIES
    // exceed capacity) and flip the enqueue path to the reject branch.
    private static void mpmc(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "mpmc");
        MpmcQueue<Long> warm = new MpmcQueue<>(ENTRIES);
        for (int i = 0; i < WARMUP; i++) {
            warm.tryEnqueue((long) i);
            warm.tryDequeue();
        }

        MpmcQueue<Long> q = new MpmcQueue<>(ENTRIES);
        int cap = q.capacity();
        SubMsPerfHarness.Stage enq = h.stage("mpmc_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            if (i >= cap) break;
            final long v = i;
            enq.time(() -> q.tryEnqueue(v));
        }
        SubMsPerfHarness.Stage deq = h.stage("mpmc_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            deq.time(q::tryDequeue);
        }
    }

    // ---------- bounded ----------
    // Same bounded-ring hazard as mpmc: warm interleaved on a throwaway so the
    // measured enqueue pass starts empty and stays on the grant path.
    private static void bounded(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "bounded");
        BoundedMpscQueue<Long> warm = new BoundedMpscQueue<>(ENTRIES);
        for (int i = 0; i < WARMUP; i++) {
            warm.tryEnqueue((long) i);
            warm.tryDequeue();
        }

        BoundedMpscQueue<Long> q = new BoundedMpscQueue<>(ENTRIES);
        int cap = q.capacity();
        SubMsPerfHarness.Stage enq = h.stage("bounded_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            if (i >= cap) break;
            final long v = i;
            enq.time(() -> q.tryEnqueue(v));
        }
        SubMsPerfHarness.Stage deq = h.stage("bounded_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            deq.time(q::tryDequeue);
        }
    }

    // ---------- batch ----------
    // Fill the queue, then drain it in BATCH-wide passes. One timed call pays
    // a single acquire-fence per drain rather than one per item; the stage
    // count is the number of batch calls.
    private static void batch(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "batch");
        // Warm the BATCH-wide drain path on a throwaway queue filled the same way.
        // Each drain moves BATCH items, so this stage is genuinely O(BATCH) per
        // call; warmup only makes the steady-state number stable, not sub-ms.
        // Re-fill and re-drain several times so the drain path reaches C2.
        Long[] warmBuf = new Long[BATCH];
        for (int round = 0; round < 16; round++) {
            BatchMpscQueue<Long> warm = new BatchMpscQueue<>();
            for (long i = 0; i < ENTRIES; i++) warm.push(i);
            while (warm.tryDequeueBatch(warmBuf) > 0) { /* drain */ }
        }

        BatchMpscQueue<Long> q = new BatchMpscQueue<>();
        for (long i = 0; i < ENTRIES; i++) {
            q.push(i);
        }
        Long[] buf = new Long[BATCH];
        int batches = (ENTRIES + BATCH - 1) / BATCH;
        SubMsPerfHarness.Stage drain = h.stage("batch_drain", batches).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < batches; i++) {
            drain.time(() -> {
                int n = q.tryDequeueBatch(buf);
                for (int j = 0; j < n; j++) buf[j] = null;
            });
        }
    }

    // ---------- metrics ----------
    private static void metrics(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "metrics");
        MetricsMpscQueue<Long> warm = new MetricsMpscQueue<>();
        for (int i = 0; i < WARMUP; i++) warm.push((long) i);
        for (int i = 0; i < WARMUP; i++) warm.tryPoll();

        MetricsMpscQueue<Long> q = new MetricsMpscQueue<>();
        SubMsPerfHarness.Stage enq = h.stage("metrics_enqueue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (long i = 0; i < ENTRIES; i++) {
            final long v = i;
            enq.time(() -> q.push(v));
        }
        SubMsPerfHarness.Stage deq = h.stage("metrics_dequeue", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            deq.time(q::tryPoll);
        }
    }

    // ---------- affinity ----------
    // No per-op hot-path cost; pins once at setup. The call is a stateless no-op
    // on the stock JDK, so warming then sampling many times is safe and gives a
    // steady-state number rather than a single cold reading.
    private static void affinity(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "affinity");
        SubMsPerfHarness.Stage set = h.stage("affinity_set", 2_000).withKind(SubMsStageKind.ONE_SHOT);
        set.warmThenTime(500, 2_000, () -> JavaAffinity.setAffinity(0));
    }

    private PerfFeaturesMain() {}
}
