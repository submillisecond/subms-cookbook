package com.submillisecond.recipes.hdrhist;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureCategory;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.hdrhist.features.Clock;
import com.submillisecond.recipes.hdrhist.features.ConcurrentHdrHistogram;
import com.submillisecond.recipes.hdrhist.features.DecayingHdrHistogram;
import com.submillisecond.recipes.hdrhist.features.DualRecorder;
import com.submillisecond.recipes.hdrhist.features.HdrIterators;
import com.submillisecond.recipes.hdrhist.features.Merge;
import com.submillisecond.recipes.hdrhist.features.TaggedHdrHistogram;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.IntConsumer;
import java.util.function.Supplier;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Each feature's representative op is swept across three PRECISIONS,
 * {@link SubMsFeatureManifest#classify} DECIDES the category from the shape of
 * that sweep, and the decision plus a measured p99-by-stage is merge-written
 * into {@code ../.subms/features/java.json}.
 *
 * <p>Significant digits is the sweep axis because it is what sets a histogram's
 * size: 3, 4 and 5 digits give 2048, 32768 and 262144 sub-buckets. A
 * {@code record} writes one bucket regardless, so it should read flat; anything
 * that folds the bucket array should climb. {@code decay}'s record does the
 * second while looking like the first.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE precision
 * and ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted
 * category is an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.hdrhist.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /**
     * 2048 / 32768 / 262144 sub-buckets, a 128x span. NOT 1/2/3 digits: that
     * gives 32 / 256 / 2048, and at 32 buckets a fold is entirely fixed per-call
     * cost, so three genuinely O(buckets) ops - the interval read, the decaying
     * record, the percentile walk - all measured flat and classified hot-path.
     */
    private static final int[] DIGITS = {3, 4, 5};
    private static final int CANON_D = DIGITS[DIGITS.length - 1];
    /** Recorded ops per measurement. Fixed across the sweep so a slope has one cause. */
    private static final int OPS = 20_000;
    /**
     * Samples per bulk op. A whole-structure call is far above the per-key
     * budget, so a distribution needs repeats rather than one shot. 256 is a
     * FLOOR, not a preference: the harness takes p99 as
     * {@code sorted[floor(0.99 * n)]}, so at n <= 100 that index IS {@code n - 1}
     * and the "p99" is the single worst sample. A structural verdict then turns
     * on whichever rep caught a page fault. 256 puts two samples above the index
     * and makes it a real percentile. Do not lower it.
     */
    private static final int BULK_REPS = 256;
    /**
     * Bulk warmup is TIME-BOXED rather than a fixed rep count. A fixed count
     * leaves the first sweep point running interpreted while every later point
     * reuses the compiled method, which reads as a curve that falls with size.
     */
    private static final long BULK_WARM_NANOS = 300_000_000L;
    private static final int BULK_WARM_MAX_REPS = 5_000;
    private static final long MAX_VALUE = 10_000_000L;
    private static final long HALFLIFE_NS = 1_000_000_000L;

    /**
     * Values spread over seven orders of magnitude, so buckets across the whole
     * range carry counts and a fold cannot skip most of the array.
     */
    private static long valueAt(int i) {
        return 1 + (i * 2_654_435_761L) % MAX_VALUE;
    }

    private static int subCount(int d) {
        return new HdrHistogram(d).subCount();
    }

    /**
     * Filled to OCCUPANCY, not to a fixed record count. {@code merge} and the
     * iterators visit NON-EMPTY buckets, so a fixed 20k values leaves 20k of them
     * occupied whether the array holds 2048 buckets or 262144 - the op stops
     * scaling and both features read flat.
     */
    private static HdrHistogram filled(int d) {
        HdrHistogram h = new HdrHistogram(d);
        int n = subCount(d);
        for (int i = 0; i < n; i++) {
            h.record(valueAt(i));
        }
        return h;
    }

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: base `record`, the per-op write path. Every feature is
        // classified against the cost of the write it is decorating.
        HdrHistogram base = new HdrHistogram(CANON_D);
        long baseP50 = keyed(i -> base.record(valueAt(i)), true);
        System.err.println("base record p50: " + baseP50 + "ns");

        concurrentWrites(manifest, baseP50);
        dualRecorder(manifest, baseP50);
        merge(manifest, baseP50);
        decay(manifest, baseP50);
        valueTagging(manifest, baseP50);
        iterators(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- concurrent-writes: atomic buckets, shared record ----------
    private static void concurrentWrites(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep("concurrent-writes/record", d -> {
            ConcurrentHdrHistogram c = new ConcurrentHdrHistogram(d);
            return keyed(i -> c.record(valueAt(i)), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        ConcurrentHdrHistogram c = new ConcurrentHdrHistogram(CANON_D);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("record", keyed(i -> c.record(valueAt(i)), false));
        p99.put("percentile", keyed(i -> c.valueAtPercentile(99.0), false));
        manifest.setFeature("concurrent-writes", dec.category(), p99, dec.reason());
    }

    // ---------- dual-recorder: hot/stable pair, swap and drain ----------
    private static void dualRecorder(SubMsFeatureManifest manifest, long baseP50) {
        // Swept on the interval read, not on `record`. The record is the same
        // atomic write the concurrent feature already covers; the swap-and-drain
        // is what a dual recorder is FOR, and it is O(buckets).
        long[][] sweep = sweep("dual-recorder/interval", d -> {
            DualRecorder r = new DualRecorder(d);
            int n = subCount(d);
            for (int i = 0; i < n; i++) {
                r.record(valueAt(i));
            }
            return bulk(() -> r, x -> x.getIntervalHistogram(), true);
        });
        // PINNED structural. The drain is O(buckets) from the source in BOTH
        // ports, but it is DESTRUCTIVE, so repeating it measures an already-empty
        // histogram rather than the drain: after the first rep each side has
        // nothing left to copy. Refilling between reps would put the refill
        // inside the timed region, which is the bug the ART port shipped. The
        // two ports disagree precisely here for that reason - Rust copies the
        // counter array unconditionally (468us at 262144 buckets) while Java
        // collapses a high-water index and reads 100ns - and neither number is
        // the operation a caller performs. Recording either as hot-path would
        // say a whole-array drain is safe per-op.
        SubMsFeatureManifest.Decision dec =
                SubMsFeatureManifest.classify(sweep, baseP50, SubMsFeatureCategory.STRUCTURAL);

        DualRecorder r = new DualRecorder(CANON_D);
        int n = subCount(CANON_D);
        for (int i = 0; i < n; i++) {
            r.record(valueAt(i));
        }
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("record", keyed(i -> r.record(valueAt(i)), false));
        p99.put("interval_read", bulk(() -> r, x -> x.getIntervalHistogram(), false));
        manifest.setFeature("dual-recorder", dec.category(), p99, dec.reason());
    }

    // ---------- merge: element-wise add over the bucket arrays ----------
    private static void merge(SubMsFeatureManifest manifest, long baseP50) {
        // Both histograms come from the supplier, OUTSIDE the timed region.
        // Repeating the merge grows the destination's counts but does identical
        // work each rep, so the figure is the merge and nothing else.
        long[][] sweep = sweep("merge/merge",
                d -> bulk(() -> pair(d), x -> Merge.merge(x[0], x[1]), true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("merge", bulk(() -> pair(CANON_D), x -> Merge.merge(x[0], x[1]), false));
        manifest.setFeature("merge", dec.category(), p99, dec.reason());
    }

    private static HdrHistogram[] pair(int d) {
        return new HdrHistogram[] {filled(d), filled(d)};
    }

    // ---------- decay: exponentially-weighted counts ----------
    private static void decay(SubMsFeatureManifest manifest, long baseP50) {
        // The clock ADVANCES on every read. A frozen clock lets the decay pass
        // early-return and the feature measures as a plain record, which is the
        // opposite of the truth: with time moving, every record first brings the
        // whole counter array up to date, so the write is O(buckets). Freezing
        // the clock here would have published that as hot-path.
        long[][] sweep = sweep("decay/record", d -> {
            DecayingHdrHistogram x = new DecayingHdrHistogram(d, HALFLIFE_NS, new TickingClock());
            return keyed(i -> x.record(valueAt(i)), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        DecayingHdrHistogram x =
                new DecayingHdrHistogram(CANON_D, HALFLIFE_NS, new TickingClock());
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("record", keyed(i -> x.record(valueAt(i)), false));
        p99.put("percentile", keyed(i -> x.valueAtPercentile(99.0), false));
        manifest.setFeature("decay", dec.category(), p99, dec.reason());
    }

    /** Advances a millisecond per read, so the decay pass never early-returns. */
    private static final class TickingClock implements Clock {
        private long now;

        @Override
        public long nowNs() {
            now += 1_000_000L;
            return now;
        }
    }

    // ---------- value-tagging: a parallel histogram per tag ----------
    private static void valueTagging(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep("value-tagging/record", d -> {
            TaggedHdrHistogram t = new TaggedHdrHistogram(d);
            return keyed(i -> t.record(valueAt(i), (byte) (i % 4)), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        TaggedHdrHistogram t = new TaggedHdrHistogram(CANON_D);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("record", keyed(i -> t.record(valueAt(i), (byte) (i % 4)), false));
        p99.put("percentile_for_tag",
                keyed(i -> t.valueAtPercentileForTag(99.0, (byte) 0), false));
        manifest.setFeature("value-tagging", dec.category(), p99, dec.reason());
    }

    // ---------- iterators: linear / logarithmic / percentile walks ----------
    private static void iterators(SubMsFeatureManifest manifest, long baseP50) {
        // A percentile walk accumulates counts across the bucket array, so it is
        // O(buckets), and the sweep is monotonic and strongly rising. It is not
        // 64x: the walk emits a BOUNDED number of entries (1% steps, ~100 of
        // them) however large the array is, so the per-bucket work amortises
        // better at the top and it measures ~57x over a 128x span - under the
        // classifier's 0.5 guard.
        //
        // `linear` was tried as the swept op instead, on the reasoning that it
        // visits every bucket. It does not: it steps by VALUE unit, so over a
        // 10^7 value range it emits millions of entries and the walk never
        // finished. Wrong op, not a slower one.
        //
        // PINNED structural rather than published as hot-path, which would tell
        // a reader a full percentile walk is safe per-operation. It is not.
        long[][] sweep = sweep("iterators/percentiles", d -> {
            HdrHistogram h = filled(d);
            return bulk(() -> h, x -> drain(HdrIterators.percentiles(x, 1.0)), true);
        });
        SubMsFeatureManifest.Decision dec =
                SubMsFeatureManifest.classify(sweep, baseP50, SubMsFeatureCategory.STRUCTURAL);

        HdrHistogram h = filled(CANON_D);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("iter_percentiles",
                bulk(() -> h, x -> drain(HdrIterators.percentiles(x, 1.0)), false));
        p99.put("iter_logarithmic", bulk(() -> h, x -> drain(HdrIterators.logarithmic(x)), false));
        manifest.setFeature("iterators", dec.category(), p99, dec.reason());
    }

    private static int drain(Iterator<?> it) {
        int n = 0;
        while (it.hasNext()) {
            it.next();
            n++;
        }
        return n;
    }

    // ---------- harness plumbing ----------

    /**
     * Sweeps and PRINTS the curve, indexed by SUB-BUCKET COUNT rather than by
     * digits - the classifier reads the size column as a magnitude.
     */
    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] rows = new long[DIGITS.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < DIGITS.length; i++) {
            rows[i][0] = subCount(DIGITS[i]);
            rows[i][1] = at.at(DIGITS[i]);
            sb.append('(').append(rows[i][0]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int d);
    }

    private static long keyed(IntConsumer op, boolean median) {
        SubMsPerfHarness h = new SubMsPerfHarness("hdr-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        // Warm to C2 first. An unwarmed JIT costs most on the FIRST measured
        // size, which the sweep reads as a cost that FALLS with N - the opposite
        // of the structural signal, and just as wrong.
        for (int i = 0; i < OPS; i++) {
            op.accept(i);
        }
        for (int i = 0; i < OPS; i++) {
            int idx = i;
            st.time(() -> op.accept(idx));
        }
        return stat(h, median);
    }

    private static <T> long bulk(Supplier<T> setup, Consumer<T> op, boolean median) {
        T input = setup.get();
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(input);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("hdr-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", BULK_REPS);
        for (int i = 0; i < BULK_REPS; i++) {
            st.time(() -> op.accept(input));
        }
        return stat(h, median);
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
