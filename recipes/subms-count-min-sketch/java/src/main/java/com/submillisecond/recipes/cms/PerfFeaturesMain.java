package com.submillisecond.recipes.cms;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.cms.features.HeavyHitters;
import com.submillisecond.recipes.cms.features.Merge;
import com.submillisecond.recipes.cms.features.WindowedCountMinSketch;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.Supplier;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Each feature's representative op is swept across three sketch WIDTHS,
 * {@link SubMsFeatureManifest#classify} DECIDES the category from the shape of
 * that sweep, and the decision plus a measured p99-by-stage is merge-written
 * into {@code ../.subms/features/java.json}.
 *
 * <p>Width is the right sweep axis for a sketch. A count-min sketch is a fixed
 * {@code depth x width} table and a per-key op touches one cell per row, so a
 * per-key cost that holds steady as width grows is hot-path; one that climbs is
 * walking the whole table. {@code tick} and {@code merge} do exactly that, and
 * only the sweep separates them from {@code add}.
 *
 * <p>The key COUNT is held constant across sweep points. Varying the table size
 * and the op count together would leave two explanations for any slope.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE width and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.cms.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int[] WIDTHS = {4_096, 32_768, 262_144};
    /**
     * Bulk ops sweep an octave higher. A whole-table op has a fixed per-call cost
     * that dominates at 4096, which COMPRESSES the measured ratio: {@code tick}
     * swept over the keyed widths reads ~30x over 64x and falls under the
     * classifier's 0.5 guard, so a genuinely O(n) op classifies flat. Starting an
     * octave up measures the asymptote rather than the call overhead.
     */
    private static final int[] BULK_WIDTHS = {32_768, 262_144, 2_097_152};

    private static final int CANON = WIDTHS[WIDTHS.length - 1];
    private static final int DEPTH = 5;
    /** Keyed ops per measurement. Fixed across the sweep - see the class note. */
    private static final int OPS = 20_000;
    /** Timed repeats for a whole-table op, far too slow to run OPS times. */
    private static final int BULK_REPS = 64;
    /**
     * Bulk warmup is TIME-BOXED, not a fixed rep count, and this is the whole
     * difference between a usable Java sweep and a worthless one. A fixed 8 reps
     * left the first sweep point running interpreted: {@code tick} measured
     * 608 us at width 32768 and 53 us at 262144 - eight times the work, an order
     * of magnitude FASTER, because by then C2 had compiled the loop. The sweep is
     * shared across sizes, so whichever point runs first eats the compilation and
     * the curve comes out non-monotonic. A budget rather than a count reaches C2
     * at every width: cheap sizes get thousands of reps, expensive ones get
     * enough, and neither stalls the run.
     */
    private static final long BULK_WARM_NANOS = 300_000_000L;
    private static final int BULK_WARM_MAX_REPS = 5_000;

    public static void main(String[] args) throws IOException {
        String[] ks = keys();

        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: a base-sketch estimate at the canonical width. A feature
        // landing at or under this costs nothing on the read path.
        CountMinSketch base = new CountMinSketch(DEPTH, CANON);
        for (String k : ks) {
            base.add(k);
        }
        long baseP50 = keyedP50(ks, base::estimate);
        System.err.println("base estimate p50: " + baseP50 + "ns");

        heavyHitters(manifest, baseP50, ks);
        windowed(manifest, baseP50, ks);
        merge(manifest, baseP50, ks);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- heavy-hitters: a top-K list kept alongside the counts ----------
    private static void heavyHitters(SubMsFeatureManifest manifest, long baseP50, String[] ks) {
        long[][] sweep = sweep("heavy-hitters/estimate", WIDTHS, w -> {
            HeavyHitters hh = new HeavyHitters(16, DEPTH, w);
            for (String k : ks) {
                hh.add(k);
            }
            return keyedP50(ks, hh::estimate);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        HeavyHitters hh = new HeavyHitters(16, DEPTH, CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("add", keyedP99(ks, hh::add));
        p99.put("estimate", keyedP99(ks, hh::estimate));
        p99.put("top_k", keyedP99(ks, k -> hh.top()));
        manifest.setFeature("heavy-hitters", d.category(), p99, d.reason());
    }

    // ---------- windowed: a ring of slices; tick clears the oldest ----------
    private static void windowed(SubMsFeatureManifest manifest, long baseP50, String[] ks) {
        // Swept on `tick`, not on `add`. `add` is O(depth) like the base and says
        // nothing about the feature; `tick` is what a windowed sketch is FOR, is
        // O(depth*width), and is the cost a reader is deciding whether to put on
        // their hot path.
        long[][] sweep = sweep("windowed/tick", BULK_WIDTHS,
                w -> bulk(() -> new WindowedCountMinSketch(4, DEPTH, w),
                        WindowedCountMinSketch::tick, true));
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        WindowedCountMinSketch win = new WindowedCountMinSketch(4, DEPTH, CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("add", keyedP99(ks, win::add));
        p99.put("estimate", keyedP99(ks, win::estimate));
        p99.put("tick", bulk(() -> new WindowedCountMinSketch(4, DEPTH, CANON),
                WindowedCountMinSketch::tick, false));
        manifest.setFeature("windowed", d.category(), p99, d.reason());
    }

    // ---------- merge: element-wise max over depth*width cells ----------
    private static void merge(SubMsFeatureManifest manifest, long baseP50, String[] ks) {
        // Both sketches come from the supplier, OUTSIDE the timed region. Merging
        // the same pair repeatedly does identical work each rep - element-wise max
        // is idempotent - so the figure is the merge and nothing else.
        long[][] sweep = sweep("merge/mergeInto", BULK_WIDTHS,
                w -> bulk(() -> pair(w, ks), p -> Merge.mergeInto(p[0], p[1]), true));
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("merge", bulk(() -> pair(CANON, ks), p -> Merge.mergeInto(p[0], p[1]), false));
        manifest.setFeature("merge", d.category(), p99, d.reason());
    }

    private static CountMinSketch[] pair(int width, String[] ks) {
        CountMinSketch dst = new CountMinSketch(DEPTH, width);
        CountMinSketch src = new CountMinSketch(DEPTH, width);
        for (String k : ks) {
            dst.add(k);
            src.add(k);
        }
        return new CountMinSketch[] {dst, src};
    }

    // ---------- harness plumbing ----------

    private static String[] keys() {
        String[] ks = new String[OPS];
        for (int i = 0; i < OPS; i++) {
            ks[i] = "key-" + i;
        }
        return ks;
    }

    /**
     * Sweeps and PRINTS the curve. A non-monotonic or ratio-compressed sweep
     * classifies flat, and the only way to catch one is to look at the rows.
     */
    private static long[][] sweep(String label, int[] widths, SizedMeasure at) {
        long[][] rows = new long[widths.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < widths.length; i++) {
            rows[i][0] = widths[i];
            rows[i][1] = at.at(widths[i]);
            sb.append('(').append(widths[i]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    /** A size-indexed measurement; IntUnaryOperator returns int, not long. */
    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n);
    }

    /**
     * p50 (ns) of {@code op} over every key. p50 rather than p99 because the
     * sweep is read as a slope, and a p99 over a few thousand samples moves on
     * one outlier - which swamps the size signal it is there to expose.
     */
    private static long keyedP50(String[] ks, Consumer<String> op) {
        return stat(keyedRun(ks, op), true);
    }

    private static long keyedP99(String[] ks, Consumer<String> op) {
        return stat(keyedRun(ks, op), false);
    }

    private static SubMsPerfHarness keyedRun(String[] ks, Consumer<String> op) {
        SubMsPerfHarness h = new SubMsPerfHarness("cms-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", ks.length);
        // Warm to C2 first. An unwarmed JIT costs most on the FIRST measured
        // size, which the sweep reads as a cost that FALLS with N - the opposite
        // of the structural signal, and just as wrong.
        for (String k : ks) {
            op.accept(k);
        }
        for (String k : ks) {
            st.time(() -> op.accept(k));
        }
        return h;
    }

    /**
     * A whole-table op. The supplier runs OUTSIDE the timed region and the warm
     * reps are discarded, for the same reason the keyed path warms: measured
     * cold, a bulk op lands its first-touch and interpreter cost on whichever
     * sweep point runs first.
     */
    private static <T> long bulk(Supplier<T> setup, Consumer<T> op, boolean median) {
        T input = setup.get();
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(input);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("cms-feature", "java");
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
