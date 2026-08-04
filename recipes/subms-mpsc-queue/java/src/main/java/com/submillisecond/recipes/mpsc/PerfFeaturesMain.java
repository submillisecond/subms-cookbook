package com.submillisecond.recipes.mpsc;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureCategory;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.mpsc.features.BatchMpscQueue;
import com.submillisecond.recipes.mpsc.features.BoundedMpscQueue;
import com.submillisecond.recipes.mpsc.features.JavaAffinity;
import com.submillisecond.recipes.mpsc.features.MetricsMpscQueue;
import com.submillisecond.recipes.mpsc.features.MpmcQueue;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Each feature's representative op is swept across three queue SIZES,
 * {@link SubMsFeatureManifest#classify} DECIDES the category from the shape of
 * that sweep, and the decision plus a measured p99-by-stage is merge-written
 * into {@code ../.subms/features/java.json}.
 *
 * <p>Resident elements is the sweep axis: for the linked variants it is the
 * number of nodes the queue holds, for the ring variants the ring's capacity
 * with half of it occupied. Enqueue and dequeue are O(1) by construction, so a
 * FLAT curve is the expected answer and a rising one would be the finding; what
 * the sweep guards against is a feature whose bookkeeping walks the queue.
 *
 * <p>Two measurement units, deliberately. The SWEEP times a sample of
 * {@code ITEMS_PER_SAMPLE} round trips, not one op: an enqueue costs a few ns
 * and the platform clock this bench was developed on ticks at 100 ns, so a
 * single-op p50 reads exactly 100 or 200 ns and the category is decided by which
 * side of a tick boundary the op lands on. {@code p99ByStage} times ONE op, the
 * way every other recipe's manifest does, so the numbers stay comparable across
 * the cookbook; those figures are published only from a fleet capture.
 *
 * <p>A sample covers the same ITEM count in every feature, {@code batch}
 * included, whose calls are {@code BATCH} items wide - cost per item moved is
 * exactly the comparison the batch feature exists to win.
 *
 * <p>The multi-producer variants are measured SINGLE-THREADED. That isolates the
 * CAS and the counter indirection from the contention they exist to survive; a
 * contended number here would say more about the thread count and the box than
 * about the feature.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it, and can disagree.
 *
 * <p>On a heterogeneous laptop, pin the JVM to one core before trusting a local
 * run: unpinned, the scheduler moves it between core clusters and the same sweep
 * point comes back anywhere between 23300 and 63400 ns. The Rust port pins
 * itself through the recipe's own {@code affinity} feature; this side cannot,
 * because the stock JDK has no pinning API, so it is done to the process from
 * outside ({@code start /affinity <mask>} on Windows, {@code taskset -c} on
 * Linux). A fleet box isolates cores already and needs neither.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.mpsc.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /** Resident elements (ring capacity for the bounded variants). A 64x span. */
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];
    /**
     * Items moved inside ONE timed sample. Fixed across every feature so the
     * base-delta test compares cost per item moved.
     */
    private static final int ITEMS_PER_SAMPLE = 1_024;
    /** Timed samples per sweep point. */
    private static final int SAMPLES = 256;
    /** Interleaved measurements per sweep point; the lowest is what gets classified. */
    private static final int SWEEP_REPEATS = 5;
    /**
     * Items per batch call. FIXED across the sweep - varying it would sweep the
     * batch size rather than the queue.
     */
    private static final int BATCH = 256;
    /** Single-op reps behind each {@code p99ByStage} figure. */
    private static final int OPS = 50_000;
    /**
     * Reps for an op that allocates a copy of the whole structure. Enough that
     * {@code floor(0.99 * n)} is a real percentile with samples above it, few
     * enough that the churn fits in the heap.
     */
    private static final int WHOLE_STRUCTURE_REPS = 512;
    /**
     * Warmup is TIME-BOXED, not a fixed rep count. A fixed count leaves the first
     * sweep point running interpreted while every later point reuses the compiled
     * method, which reads as a curve that FALLS with size - as wrong as a fake
     * rise, and harder to spot.
     */
    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_MAX_SAMPLES = 5_000;
    /**
     * One-off burn before the first measurement, so the process-level ramp - C2
     * compilation of the shared harness path, the first heap expansions, the
     * clock-boost ramp - lands somewhere other than the first sweep point.
     */
    private static final long BURN_NANOS = 2_000_000_000L;

    /** Keeps dequeued values reachable so nothing measured is dead code. */
    private static Object sink;
    /** Same, for the batch drain's returned count. */
    private static long drained;
    /**
     * One pre-boxed element, pushed by every timed op. Boxing a fresh Long per
     * push allocates a SECOND object per enqueue that the Rust port has no
     * counterpart for, and it is what put the sweep at the mercy of the young
     * collector: the base curve read 29000 / 54000 / 33700 ns, a peak in the
     * middle, because a mid-sized live set gets copied between survivor spaces on
     * every young GC while a large one is promoted out of the way. The queue
     * op is what is being measured, not the boxing in front of it.
     */
    private static final Long VALUE = 4_242L;

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        burn();

        // The baseline: the base queue's push + tryPoll round trip. Swept as well
        // as sampled, because whether queue depth moves the BASE op is the
        // context every feature curve is read against.
        long[][] baseSweep = sweep("base/push+poll", n -> {
            MpscQueue<Long> q = filled(n);
            return batched(ITEMS_PER_SAMPLE, i -> {
                q.push(VALUE);
                sink = q.tryPoll();
            });
        });
        long baseP50 = baseSweep[SIZES.length - 1][1];
        System.err.println(
                "base push+poll p50 per " + ITEMS_PER_SAMPLE + "-item sample: " + baseP50 + "ns");

        bounded(manifest, baseP50);
        mpmc(manifest, baseP50);
        batch(manifest, baseP50);
        metrics(manifest, baseP50);
        affinity(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    /**
     * Every {@code batched} call warms itself, but the FIRST measurement in the
     * process pays a ramp the per-measurement warm sits inside rather than
     * absorbs, and the sweep runs smallest-first. Without this the Rust port's
     * base curve read 71800 / 45300 / 46500 ns - a 1.6x fall with size that is
     * the process settling, not the queue.
     */
    private static void burn() {
        MpscQueue<Long> q = filled(CANON);
        long deadline = System.nanoTime() + BURN_NANOS;
        while (System.nanoTime() < deadline) {
            for (int i = 0; i < ITEMS_PER_SAMPLE; i++) {
                q.push(VALUE);
                sink = q.tryPoll();
            }
        }
    }

    // ---------- bounded: fixed-capacity ring, backpressure on enqueue ----------
    private static void bounded(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("bounded/enqueue+dequeue", n -> {
            BoundedMpscQueue<Long> q = ring(n);
            return batched(ITEMS_PER_SAMPLE, i -> {
                q.tryEnqueue(VALUE);
                sink = q.tryDequeue();
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        BoundedMpscQueue<Long> q = ring(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("enqueue", single((st, i) -> {
            st.time(() -> q.tryEnqueue(VALUE));
            sink = q.tryDequeue();
        }));
        p99.put("dequeue", single((st, i) -> {
            q.tryEnqueue(VALUE);
            st.time(() -> sink = q.tryDequeue());
        }));
        // The reject path, which is the reason the feature exists. The ring is
        // filled to capacity once, OUTSIDE the timed region; every timed call
        // then takes the full branch.
        BoundedMpscQueue<Long> full = new BoundedMpscQueue<>(CANON);
        while (full.tryEnqueue(VALUE)) {
            // fill to capacity
        }
        p99.put("enqueue_full", single((st, i) -> st.time(() -> full.tryEnqueue(VALUE))));
        manifest.setFeature("bounded", dec.category(), p99, dec.reason());
    }

    /**
     * Half full at every sweep point. Filled to a FIXED element count instead,
     * the big rings would sit 98% empty and the enqueue would be measuring the
     * fill fraction rather than the footprint.
     */
    private static BoundedMpscQueue<Long> ring(int n) {
        BoundedMpscQueue<Long> q = new BoundedMpscQueue<>(n);
        for (int i = 0; i < n / 2; i++) {
            q.tryEnqueue(VALUE);
        }
        return q;
    }

    // ---------- mpmc: bounded ring, sequence CAS on both ends ----------
    private static void mpmc(SubMsFeatureManifest manifest, long baseP50) {
        // Uncontended, so every CAS succeeds first try. That is the figure the
        // category is about: what the multi-consumer claim costs a queue that is
        // NOT contended, which is the state a well-sized pipeline runs in.
        long[][] sw = sweep("mpmc/enqueue+dequeue", n -> {
            MpmcQueue<Long> q = mpmcRing(n);
            return batched(ITEMS_PER_SAMPLE, i -> {
                q.tryEnqueue(VALUE);
                sink = q.tryDequeue();
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        MpmcQueue<Long> q = mpmcRing(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("enqueue", single((st, i) -> {
            st.time(() -> q.tryEnqueue(VALUE));
            sink = q.tryDequeue();
        }));
        p99.put("dequeue", single((st, i) -> {
            q.tryEnqueue(VALUE);
            st.time(() -> sink = q.tryDequeue());
        }));
        manifest.setFeature("mpmc", dec.category(), p99, dec.reason());
    }

    private static MpmcQueue<Long> mpmcRing(int n) {
        MpmcQueue<Long> q = new MpmcQueue<>(n);
        for (int i = 0; i < n / 2; i++) {
            q.tryEnqueue(VALUE);
        }
        return q;
    }

    // ---------- batch: drain up to BATCH items behind one acquire fence ----------
    private static void batch(SubMsFeatureManifest manifest, long baseP50) {
        // A sample moves ITEMS_PER_SAMPLE items either way; only the call width
        // differs. That is why the reps count is divided rather than the batch
        // grown - growing it would sweep the batch size, and the number would
        // stop being comparable to the base round trip.
        long[][] sw = sweep("batch/push+dequeueBatch", n -> {
            BatchMpscQueue<Long> q = filledBatch(n);
            Long[] buf = new Long[BATCH];
            return batched(ITEMS_PER_SAMPLE / BATCH, i -> {
                for (int j = 0; j < BATCH; j++) {
                    q.push(VALUE);
                }
                drained += q.tryDequeueBatch(buf);
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        BatchMpscQueue<Long> q = filledBatch(CANON);
        Long[] buf = new Long[BATCH];
        Map<String, Long> p99 = new LinkedHashMap<>();
        // The refill is outside the timed region: timing it would put a BATCH of
        // pushes inside the drain's number and the stage would stop being a drain
        // figure at all.
        p99.put("dequeue_batch", single((st, i) -> {
            st.time(() -> drained += q.tryDequeueBatch(buf));
            for (int j = 0; j < BATCH; j++) {
                q.push(VALUE);
            }
        }));
        p99.put("enqueue", single((st, i) -> {
            st.time(() -> q.push(VALUE));
            q.tryDequeueBatch(buf, 1);
        }));
        manifest.setFeature("batch", dec.category(), p99, dec.reason());
    }

    private static BatchMpscQueue<Long> filledBatch(int n) {
        BatchMpscQueue<Long> q = new BatchMpscQueue<>();
        for (int i = 0; i < n; i++) {
            q.push(VALUE);
        }
        return q;
    }

    // ---------- metrics: relaxed atomic counters around each op ----------
    private static void metrics(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("metrics/push+poll", n -> {
            MetricsMpscQueue<Long> q = filledMetrics(n);
            return batched(ITEMS_PER_SAMPLE, i -> {
                q.push(VALUE);
                sink = q.tryPoll();
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        MetricsMpscQueue<Long> q = filledMetrics(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("enqueue", single((st, i) -> {
            st.time(() -> q.push(VALUE));
            sink = q.tryPoll();
        }));
        p99.put("dequeue", single((st, i) -> {
            q.push(VALUE);
            st.time(() -> sink = q.tryPoll());
        }));
        p99.put("snapshot", single(WHOLE_STRUCTURE_REPS, (st, i) -> st.time(() -> sink = q.snapshot())));
        manifest.setFeature("metrics", dec.category(), p99, dec.reason());
    }

    private static MetricsMpscQueue<Long> filledMetrics(int n) {
        MetricsMpscQueue<Long> q = new MetricsMpscQueue<>();
        for (int i = 0; i < n; i++) {
            q.push(VALUE);
        }
        return q;
    }

    // ---------- affinity: pin the calling thread, once, at startup ----------
    private static void affinity(SubMsFeatureManifest manifest, long baseP50) {
        // Swept over the same axis to show what it is: a call that touches no
        // queue state and cannot move with queue size. PINNED auxiliary rather
        // than left to the base-delta test, because the call is made once per
        // thread at startup and appears in neither push nor tryPoll, whatever it
        // costs. The two ports are not measuring the same thing here either:
        // Rust issues a real SetThreadAffinityMask / sched_setaffinity, while
        // this side validates its argument and returns UNSUPPORTED - the stock
        // JDK has no pinning API - so the Java figure is an argument check and
        // the Rust one is a syscall. Pinning the category is what keeps that
        // asymmetry out of the manifest.
        long[][] sw = sweep("affinity/setAffinity",
                n -> batched(ITEMS_PER_SAMPLE, i -> sink = JavaAffinity.setAffinity(0)));
        SubMsFeatureManifest.Decision dec =
                SubMsFeatureManifest.classify(sw, baseP50, SubMsFeatureCategory.AUXILIARY);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("set_affinity",
                single((st, i) -> st.time(() -> sink = JavaAffinity.setAffinity(0))));
        manifest.setFeature("affinity", dec.category(), p99, dec.reason());
    }

    // ---------- harness plumbing ----------

    private static MpscQueue<Long> filled(int n) {
        MpscQueue<Long> q = new MpscQueue<>();
        for (int i = 0; i < n; i++) {
            q.push(VALUE);
        }
        return q;
    }

    /**
     * Sweeps and PRINTS the curve, both the figures it returns and the raw
     * repeats behind them. A ratio-compressed or non-monotonic curve classifies
     * flat, and the rows are the only place that shows up.
     *
     * <p>Every size is measured {@code SWEEP_REPEATS} times, INTERLEAVED (all
     * three sizes, then all three again), and the LOWEST is taken. Measuring one
     * size to completion before moving on aliases slow drift onto the size axis,
     * and there is plenty of it here: a collection, a deoptimisation, or a
     * migration between core clusters all outlast a single measurement and are
     * indistinguishable from scaling if they land inside one sweep point.
     * Interleaving spreads them across all three points, and the minimum takes
     * the least disturbed run - disturbance is one-sided, so an average carries
     * the interference into the comparison while the minimum compares the queues.
     * The p99 figures in {@code p99ByStage} still come from an ordinary timed
     * pass, interference included; it is the CATEGORY decision, not the published
     * latency, that wants the undisturbed number.
     */
    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] runs = new long[SIZES.length][SWEEP_REPEATS];
        for (int r = 0; r < SWEEP_REPEATS; r++) {
            for (int i = 0; i < SIZES.length; i++) {
                runs[i][r] = at.at(SIZES[i]);
            }
        }
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        StringBuilder raw = new StringBuilder(" repeats ");
        for (int i = 0; i < SIZES.length; i++) {
            raw.append(java.util.Arrays.toString(runs[i]));
            long lowest = runs[i][0];
            for (long v : runs[i]) {
                lowest = Math.min(lowest, v);
            }
            rows[i][0] = SIZES[i];
            rows[i][1] = lowest;
            sb.append('(').append(rows[i][0]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim() + raw);
        return rows;
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n);
    }

    @FunctionalInterface
    private interface IndexedOp {
        void run(int i);
    }

    @FunctionalInterface
    private interface Body {
        void run(SubMsPerfHarness.Stage st, int i);
    }

    /** p50 ns of one timed sample covering {@code reps} calls of {@code op}. */
    private static long batched(int reps, IndexedOp op) {
        int[] idx = {0};
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int s = 0; s < WARM_MAX_SAMPLES && System.nanoTime() < deadline; s++) {
            for (int k = 0; k < reps; k++) {
                op.run(idx[0]++);
            }
        }
        SubMsPerfHarness h = new SubMsPerfHarness("mpsc-queue-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", SAMPLES);
        for (int s = 0; s < SAMPLES; s++) {
            st.time(() -> {
                for (int k = 0; k < reps; k++) {
                    op.run(idx[0]++);
                }
            });
        }
        return stat(h, true);
    }

    /**
     * p99 ns of a single timed op. {@code body} is handed the stage so it can put
     * the restoring half of the round trip OUTSIDE the timed region: without
     * that, a 50k-op enqueue pass drifts the queue off its steady-state depth and
     * a bounded ring ends up measuring its full branch instead of the fast path.
     */
    private static long single(Body body) {
        return single(OPS, body);
    }

    /**
     * {@link #single(Body)} with an explicit rep count, for an op whose COST PER
     * REP is a whole-structure allocation rather than a per-element one.
     *
     * <p>{@code snapshot} copies the entire queue, so at {@code CANON} it hands
     * back a 262k-element array per call. Warm plus measured at {@code OPS} is
     * 100k of those - a megabyte-scale array each, allocated straight out of the
     * young gen - and the JVM exhausts the heap long before the run ends. The
     * Rust port measures exactly the same thing at the same rep count and
     * survives, because it frees each snapshot immediately; there is no garbage
     * to outrun. A legitimate difference between the runtimes, not a parity
     * break - the two ports still measure per-op snapshot latency, and both
     * still compute a real p99 (floor(0.99 * n) leaves samples above it at this
     * count).
     */
    private static long single(int reps, Body body) {
        SubMsPerfHarness warm = new SubMsPerfHarness("mpsc-queue-feature", "java");
        SubMsPerfHarness.Stage ws = warm.stage("op", reps);
        for (int i = 0; i < reps; i++) {
            body.run(ws, i);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("mpsc-queue-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", reps);
        for (int i = 0; i < reps; i++) {
            body.run(st, i);
        }
        return stat(h, false);
    }

    private static long stat(SubMsPerfHarness h, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }

    private PerfFeaturesMain() {}
}
