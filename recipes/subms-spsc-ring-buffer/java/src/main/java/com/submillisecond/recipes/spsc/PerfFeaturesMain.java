package com.submillisecond.recipes.spsc;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.spsc.features.Bulk;
import com.submillisecond.recipes.spsc.features.Metrics;
import com.submillisecond.recipes.spsc.features.MpmcDisruptor;
import com.submillisecond.recipes.spsc.features.MpscFanIn;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BlockingSpscConsumer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BlockingSpscProducer;
import com.submillisecond.recipes.spsc.features.WaitStrategies.BusySpin;
import com.submillisecond.recipes.spsc.features.WaitStrategies.ParkStrategy;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Feature classification bench, the Java mirror of
 * {@code rust/examples/perf_features.rs}. Each feature's representative op is
 * swept across three RING CAPACITIES, {@link SubMsFeatureManifest#classify}
 * DECIDES the category from the shape of that sweep, and the decision plus a
 * measured p99-by-stage is merge-written into {@code ../.subms/features/java.json}.
 *
 * <p>Capacity is the sweep axis because it is the only thing that sets a ring's
 * size. Push and pop are O(1) by construction, so a flat curve is the EXPECTED
 * answer here and a rising one would be the finding; what the sweep guards
 * against is a feature whose bookkeeping walks the ring.
 *
 * <p>Two measurement units, deliberately. The SWEEP times a sample of
 * {@code ITEMS_PER_SAMPLE} round trips rather than one op: a push costs a few ns
 * and the platform clock this bench is developed on ticks at 100 ns, so every
 * single-op p50 reads exactly 100 ns and every curve is flat by quantisation
 * rather than by physics. {@code p99ByStage} times ONE op, the way every other
 * recipe's manifest does, so the numbers stay comparable across the cookbook;
 * those figures are only published from a fleet capture.
 *
 * <p>A sample covers the same ITEM count in every feature, including
 * {@code bulk}, whose calls are {@code BULK_BATCH} items wide. Comparing a
 * 32-item bulk call against a 1-item push would compare batch sizes, not
 * features.
 *
 * <p>Every ring is pre-filled to half capacity and every measured op is a round
 * trip, so occupancy is a fixed fraction at every sweep point and neither side
 * ever takes its full / empty branch. Values come from a pre-boxed pool: pushing
 * {@code (long) i} boxes a fresh {@code Long} per op, which measures the
 * allocator on the port that has one and nothing on the port that does not.
 *
 * <p>Each sweep point is measured {@code ROUNDS} times, size-interleaved, and
 * the MINIMUM is kept - the Rust port found every measurement landing on one of
 * two clock levels a constant 1.31x apart, which a median mixes into a fake size
 * trend.
 *
 * <p>{@code wait-strategies} is measured ONLY on the non-full, non-empty fast
 * path. A strategy's {@code waitOnce()} is a scheduler measurement -
 * {@code ParkStrategy} sleeps until the other end unparks it, which is
 * milliseconds - and publishing that as the feature's per-op cost would be a
 * category error.
 *
 * <p>The multi-producer features are measured SINGLE-THREADED at a fixed
 * producer / consumer count, which isolates the indirection from the contention
 * it exists to relieve.
 *
 * <p>Run it in its own JVM. {@code mvn exec:java} runs the bench inside Maven's
 * JVM, sharing a heap and a GC with the build, and that showed up: the same
 * sweep read 20-35% wider spread per point and turned two curves non-monotonic.
 *
 * <pre>
 *   mvn -o -q dependency:build-classpath -Dmdep.outputFile=target/cp.txt
 *   java -Xms1g -Xmx1g -cp "target/classes;$(cat target/cp.txt)" \
 *       com.submillisecond.recipes.spsc.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /** Slot counts the sweep walks. A 256x span, already powers of two. */
    private static final int[] SIZES = {1_024, 16_384, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];
    /**
     * Items moved inside ONE timed sample. Fixed across every feature so the
     * base-delta test compares cost per item moved.
     */
    private static final int ITEMS_PER_SAMPLE = 1_024;
    /** Timed samples per sweep point. */
    private static final int SAMPLES = 256;
    /** Size-interleaved repeats of the whole sweep; the per-size minimum is kept. */
    private static final int ROUNDS = 7;
    /** Items per bulk call. FIXED across the sweep. */
    private static final int BULK_BATCH = 32;
    /** Single-op reps behind each p99ByStage figure. */
    private static final int OPS = 50_000;
    /**
     * Warmup is TIME-BOXED, not a fixed rep count. A fixed count leaves the first
     * sweep point running interpreted while every later point reuses the compiled
     * method, which reads as a curve that FALLS with size.
     */
    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_MAX_SAMPLES = 1_000;
    /** Producer count for the fan-in, held FIXED across the sweep. */
    private static final int PRODUCERS = 4;
    /** Consumer count for the disruptor, held FIXED across the sweep. */
    private static final int CONSUMERS = 1;

    /** Pre-boxed values, cycled by index. Keeps the allocator out of the loop. */
    private static final int POOL = 1_024;
    private static final int POOL_MASK = POOL - 1;
    private static final Long[] VALUES = new Long[POOL];

    static {
        for (int i = 0; i < POOL; i++) {
            VALUES[i] = (long) i;
        }
    }

    /** Keeps the accumulated result live so the timed loop cannot be folded away. */
    private static volatile long sink;

    @FunctionalInterface
    private interface Op {
        long at(int i);
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int cap);
    }

    private record Ring(SpscRingBuffer<Long>.Producer tx, SpscRingBuffer<Long>.Consumer rx) {}

    private record FanIn(List<MpscFanIn<Long>.Producer> ps, MpscFanIn<Long>.Consumer c) {}

    private record Dis(MpmcDisruptor<Long>.Producer p, MpmcDisruptor<Long>.Consumer c) {}

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: the base wait-free push + pop round trip. Swept as well
        // as sampled, because whether ring capacity moves the BASE op is the
        // context every feature curve is read against.
        long[][] baseSweep = sweep("base/push+pop", cap -> {
            Ring g = ring(cap);
            return batched(ITEMS_PER_SAMPLE, i -> {
                g.tx().tryPush(VALUES[i & POOL_MASK]);
                Long v = g.rx().tryPop();
                return v == null ? 0 : v;
            });
        });
        long baseP50 = baseSweep[SIZES.length - 1][1];
        System.err.println(
                "base push+pop p50 per " + ITEMS_PER_SAMPLE + "-item sample: " + baseP50 + "ns");

        bulk(manifest, baseP50);
        waitStrategies(manifest, baseP50);
        mpscFanIn(manifest, baseP50);
        mpmcDisruptor(manifest, baseP50);
        metrics(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- bulk: one fence per BULK_BATCH items ----------
    private static void bulk(SubMsFeatureManifest manifest, long baseP50) {
        // A sample moves ITEMS_PER_SAMPLE items either way; only the call width
        // differs. That is the comparison the feature exists to win, and it is
        // why the reps count is divided rather than the batch grown.
        long[][] sw = sweep("bulk/enqueue+dequeue", cap -> {
            Ring g = ring(cap);
            Long[] batch = batch();
            Long[] out = new Long[BULK_BATCH];
            return batched(ITEMS_PER_SAMPLE / BULK_BATCH,
                    i -> Bulk.tryEnqueueBulk(g.tx(), batch) + Bulk.tryDequeueBulk(g.rx(), out));
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Long[] batch = batch();
        Long[] out = new Long[BULK_BATCH];
        Ring a = ring(CANON);
        long enq = single(i -> Bulk.tryEnqueueBulk(a.tx(), batch),
                i -> Bulk.tryDequeueBulk(a.rx(), out));
        Ring b = ring(CANON);
        long deq = single(i -> Bulk.tryDequeueBulk(b.rx(), out),
                i -> Bulk.tryEnqueueBulk(b.tx(), batch));

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("enqueue_bulk", enq);
        p99.put("dequeue_bulk", deq);
        manifest.setFeature("bulk", dec.category(), p99, dec.reason());
    }

    private static Long[] batch() {
        Long[] batch = new Long[BULK_BATCH];
        System.arraycopy(VALUES, 0, batch, 0, BULK_BATCH);
        return batch;
    }

    // ---------- wait-strategies: blocking wrappers, fast path only ----------
    private static void waitStrategies(SubMsFeatureManifest manifest, long baseP50) {
        // Swept on BusySpin, whose signal() is a no-op, so the curve is the
        // wrapper's own overhead over the base ring and nothing else. The ring is
        // never full or empty, so waitOnce() is never called; the parked-wakeup
        // path is a scheduler latency, not a per-op cost, and is not measured.
        long[][] sw = sweep("wait-strategies/push+pop", cap -> {
            Ring g = ring(cap);
            BlockingSpscProducer<Long> p = new BlockingSpscProducer<>(g.tx(), new BusySpin());
            BlockingSpscConsumer<Long> c = new BlockingSpscConsumer<>(g.rx(), new BusySpin());
            return batched(ITEMS_PER_SAMPLE, i -> {
                p.push(VALUES[i & POOL_MASK]);
                return c.pop();
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        // ParkStrategy on the SAME ready ring, printed rather than classified.
        // Its signal() fires on every successful op even when nothing is parked,
        // so the strategy choice shows up here on the fast path - not in
        // waitOnce(), which this never enters.
        long parkBatched = best(() -> {
            Ring g = ring(CANON);
            ParkStrategy[] ps = ParkStrategy.pair();
            BlockingSpscProducer<Long> p = new BlockingSpscProducer<>(g.tx(), ps[0]);
            BlockingSpscConsumer<Long> c = new BlockingSpscConsumer<>(g.rx(), ps[1]);
            return batched(ITEMS_PER_SAMPLE, i -> {
                p.push(VALUES[i & POOL_MASK]);
                return c.pop();
            });
        });
        System.err.println("wait-strategies park fast path at " + CANON + ": " + parkBatched
                + "ns per " + ITEMS_PER_SAMPLE + "-item sample (spin "
                + sw[SIZES.length - 1][1] + "ns) - signal() takes a lock per op");

        Ring a = ring(CANON);
        BlockingSpscProducer<Long> pa = new BlockingSpscProducer<>(a.tx(), new BusySpin());
        BlockingSpscConsumer<Long> ca = new BlockingSpscConsumer<>(a.rx(), new BusySpin());
        long pushSpin = single(i -> {
            pa.push(VALUES[i & POOL_MASK]);
            return 0;
        }, i -> ca.pop());

        Ring b = ring(CANON);
        BlockingSpscProducer<Long> pb = new BlockingSpscProducer<>(b.tx(), new BusySpin());
        BlockingSpscConsumer<Long> cb = new BlockingSpscConsumer<>(b.rx(), new BusySpin());
        long popSpin = single(i -> cb.pop(), i -> {
            pb.push(VALUES[i & POOL_MASK]);
            return 0;
        });

        Ring c = ring(CANON);
        ParkStrategy[] psc = ParkStrategy.pair();
        BlockingSpscProducer<Long> pc = new BlockingSpscProducer<>(c.tx(), psc[0]);
        BlockingSpscConsumer<Long> cc = new BlockingSpscConsumer<>(c.rx(), psc[1]);
        long pushPark = single(i -> {
            pc.push(VALUES[i & POOL_MASK]);
            return 0;
        }, i -> cc.pop());

        Ring d = ring(CANON);
        ParkStrategy[] psd = ParkStrategy.pair();
        BlockingSpscProducer<Long> pd = new BlockingSpscProducer<>(d.tx(), psd[0]);
        BlockingSpscConsumer<Long> cd = new BlockingSpscConsumer<>(d.rx(), psd[1]);
        long popPark = single(i -> cd.pop(), i -> {
            pd.push(VALUES[i & POOL_MASK]);
            return 0;
        });

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("push_spin", pushSpin);
        p99.put("pop_spin", popSpin);
        p99.put("push_park", pushPark);
        p99.put("pop_park", popPark);
        manifest.setFeature("wait-strategies", dec.category(), p99, dec.reason());
    }

    // ---------- mpsc-fan-in: N rings, one round-robin consumer ----------
    private static void mpscFanIn(SubMsFeatureManifest manifest, long baseP50) {
        // Pushes round-robin and the consumer cursor advances one ring per pop,
        // so the two stay in step and every ring holds a constant half load. With
        // every ring non-empty the consumer's probe hits on its first try, which
        // is the steady-state shape; a starved fan-in probes all N and that is a
        // different measurement.
        long[][] sw = sweep("mpsc-fan-in/push+pop", cap -> {
            FanIn f = fanin(cap);
            return batched(ITEMS_PER_SAMPLE, i -> {
                f.ps().get(i % PRODUCERS).tryPush(VALUES[i & POOL_MASK]);
                Long v = f.c().tryPop();
                return v == null ? 0 : v;
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        FanIn a = fanin(CANON);
        long enq = single(i -> {
            a.ps().get(i % PRODUCERS).tryPush(VALUES[i & POOL_MASK]);
            return 0;
        }, i -> {
            Long v = a.c().tryPop();
            return v == null ? 0 : v;
        });
        FanIn b = fanin(CANON);
        long deq = single(i -> {
            Long v = b.c().tryPop();
            return v == null ? 0 : v;
        }, i -> {
            b.ps().get(i % PRODUCERS).tryPush(VALUES[i & POOL_MASK]);
            return 0;
        });

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("fanin_enqueue", enq);
        p99.put("fanin_dequeue", deq);
        manifest.setFeature("mpsc-fan-in", dec.category(), p99, dec.reason());
    }

    // ---------- mpmc-disruptor: CAS claim + sequence barrier ----------
    private static void mpmcDisruptor(SubMsFeatureManifest manifest, long baseP50) {
        // Half a ring of published-but-unconsumed items keeps the producer clear
        // of the gating spin (it only fires within one capacity of the slowest
        // consumer) and the consumer clear of the unpublished early return.
        long[][] sw = sweep("mpmc-disruptor/publish+consume", cap -> {
            Dis d = disruptor(cap);
            return batched(ITEMS_PER_SAMPLE, i -> {
                d.p().tryPublish(VALUES[i & POOL_MASK]);
                Long v = d.c().tryConsume();
                return v == null ? 0 : v;
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Dis a = disruptor(CANON);
        long published = single(i -> {
            a.p().tryPublish(VALUES[i & POOL_MASK]);
            return 0;
        }, i -> {
            Long v = a.c().tryConsume();
            return v == null ? 0 : v;
        });
        Dis b = disruptor(CANON);
        long consumed = single(i -> {
            Long v = b.c().tryConsume();
            return v == null ? 0 : v;
        }, i -> {
            b.p().tryPublish(VALUES[i & POOL_MASK]);
            return 0;
        });

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("publish", published);
        p99.put("consume", consumed);
        manifest.setFeature("mpmc-disruptor", dec.category(), p99, dec.reason());
    }

    // ---------- metrics: counters on the push / pop path ----------
    private static void metrics(SubMsFeatureManifest manifest, long baseP50) {
        // The wrapper adds an increment per op plus, on the producer side, a
        // high-water-mark update. localDepth only ever increments - a producer
        // cannot observe pops - so that update takes its CAS on every push rather
        // than settling once the mark stops moving. The counter cost on the push
        // path is two read-modify-writes, not one.
        long[][] sw = sweep("metrics/push+pop", cap -> {
            Ring g = ring(cap);
            Metrics.Instrumented<Long> m = new Metrics.Instrumented<>(g.tx(), g.rx());
            return batched(ITEMS_PER_SAMPLE, i -> {
                m.producer.tryPush(VALUES[i & POOL_MASK]);
                Long v = m.consumer.tryPop();
                return v == null ? 0 : v;
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Ring ga = ring(CANON);
        Metrics.Instrumented<Long> ma = new Metrics.Instrumented<>(ga.tx(), ga.rx());
        long enq = single(i -> {
            ma.producer.tryPush(VALUES[i & POOL_MASK]);
            return 0;
        }, i -> {
            Long v = ma.consumer.tryPop();
            return v == null ? 0 : v;
        });
        Ring gb = ring(CANON);
        Metrics.Instrumented<Long> mb = new Metrics.Instrumented<>(gb.tx(), gb.rx());
        long deq = single(i -> {
            Long v = mb.consumer.tryPop();
            return v == null ? 0 : v;
        }, i -> {
            mb.producer.tryPush(VALUES[i & POOL_MASK]);
            return 0;
        });

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("metrics_enqueue", enq);
        p99.put("metrics_dequeue", deq);
        manifest.setFeature("metrics", dec.category(), p99, dec.reason());
    }

    // ---------- fixtures ----------

    /**
     * A ring pre-filled to half capacity. Occupancy is a constant fraction at
     * every sweep point, so a slope has one cause; and both sides stay off their
     * full / empty branch for the whole measurement.
     */
    private static Ring ring(int cap) {
        SpscRingBuffer<Long> b = new SpscRingBuffer<>(cap);
        SpscRingBuffer<Long>.Producer tx = b.producer();
        SpscRingBuffer<Long>.Consumer rx = b.consumer();
        for (int i = 0; i < cap / 2; i++) {
            tx.tryPush(VALUES[i & POOL_MASK]);
        }
        return new Ring(tx, rx);
    }

    private static FanIn fanin(int cap) {
        MpscFanIn<Long> f = new MpscFanIn<>(PRODUCERS, cap);
        List<MpscFanIn<Long>.Producer> ps = new ArrayList<>(PRODUCERS);
        for (int i = 0; i < PRODUCERS; i++) {
            ps.add(f.producer(i));
        }
        for (MpscFanIn<Long>.Producer p : ps) {
            for (int i = 0; i < cap / 2; i++) {
                p.tryPush(VALUES[i & POOL_MASK]);
            }
        }
        return new FanIn(ps, f.consumer());
    }

    private static Dis disruptor(int cap) {
        MpmcDisruptor<Long> d = new MpmcDisruptor<>(cap, CONSUMERS);
        MpmcDisruptor<Long>.Producer p = d.producer();
        for (int i = 0; i < cap / 2; i++) {
            p.tryPublish(VALUES[i & POOL_MASK]);
        }
        return new Dis(p, d.consumer(0));
    }

    // ---------- harness plumbing ----------

    /**
     * Sweeps and PRINTS the curve, raw rounds included in ROUND ORDER. A
     * ratio-compressed or non-monotonic curve classifies flat, and the rows are
     * the only place that shows up.
     */
    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] runs = new long[SIZES.length][ROUNDS];
        for (int r = 0; r < ROUNDS; r++) {
            for (int k = 0; k < SIZES.length; k++) {
                runs[k][r] = at.at(SIZES[k]);
            }
        }
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(":");
        for (int k = 0; k < SIZES.length; k++) {
            long m = Long.MAX_VALUE;
            for (long v : runs[k]) {
                m = Math.min(m, v);
            }
            rows[k][0] = SIZES[k];
            rows[k][1] = m;
            sb.append(" (").append(SIZES[k]).append(", ").append(m).append(')');
        }
        sb.append(" raw ").append(Arrays.deepToString(runs));
        System.err.println(sb);
        return rows;
    }

    /** Lowest of ROUNDS repeats, for a figure that is printed rather than swept. */
    private static long best(java.util.function.LongSupplier f) {
        long m = Long.MAX_VALUE;
        for (int r = 0; r < ROUNDS; r++) {
            m = Math.min(m, f.getAsLong());
        }
        return m;
    }

    /** p50 ns of one timed sample covering {@code reps} calls of {@code op}. */
    private static long batched(int reps, Op op) {
        int i = 0;
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int s = 0; s < WARM_MAX_SAMPLES && System.nanoTime() < deadline; s++) {
            long acc = 0;
            for (int r = 0; r < reps; r++) {
                acc += op.at(i++);
            }
            sink = acc;
        }
        SubMsPerfHarness h = new SubMsPerfHarness("spsc-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", SAMPLES);
        for (int s = 0; s < SAMPLES; s++) {
            final int start = i;
            st.time(() -> {
                long acc = 0;
                for (int r = 0; r < reps; r++) {
                    acc += op.at(start + r);
                }
                sink = acc;
            });
            i += reps;
        }
        return stat(h, true);
    }

    /**
     * p99 ns of a single {@code timed} call. {@code untimed} runs outside the
     * timed region and restores the ring's depth, so a 50k-op enqueue pass cannot
     * fill the ring and start measuring the full branch instead of the fast path.
     */
    private static long single(Op timed, Op untimed) {
        for (int i = 0; i < OPS; i++) {
            sink = timed.at(i);
            sink = untimed.at(i);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("spsc-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        for (int i = 0; i < OPS; i++) {
            final int k = i;
            st.time(() -> {
                sink = timed.at(k);
            });
            sink = untimed.at(k);
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
