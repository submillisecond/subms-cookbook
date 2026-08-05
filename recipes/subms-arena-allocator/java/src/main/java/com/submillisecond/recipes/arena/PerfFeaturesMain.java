package com.submillisecond.recipes.arena;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.arena.features.AlignedArena;
import com.submillisecond.recipes.arena.features.GrowableArena;
import com.submillisecond.recipes.arena.features.StatsArena;
import com.submillisecond.recipes.arena.features.TypedArena;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.IntConsumer;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Sweeps each feature (typed, growable, stats, aligned) across three
 * allocation counts, lets {@link SubMsFeatureManifest#classify} DECIDE the
 * category from the shape of that sweep, and merge-writes the decision into
 * {@code ../.subms/features/java.json} - preserving any field already there.
 *
 * <p>An arena's "size" is how many allocations it is carrying, so the sweep
 * fills to N and times the allocate path there. A per-op cost that holds steady
 * as N grows is hot-path; one that climbs with N is structural.
 *
 * <p>The sweep classifies on p50. p99 over a few dozen samples is just the worst
 * one, and a single scheduler slice is large enough to swamp the size signal the
 * sweep is reading. The p99 still goes into the manifest for the stage table.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it, and can disagree.
 *
 * <p>These p99 figures describe THIS machine. They are published only when the
 * manifest is stamped {@code p99_source: fleet}; a local run leaves the category,
 * which is machine independent, and no published number.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.arena.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /** Allocation counts the sweep walks. Mirrors SIZES in the Rust port. */
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        typed(manifest);
        growable(manifest);
        stats(manifest);
        aligned(manifest);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- typed: one type, slot handles, reuse on free ----------
    private static void typed(SubMsFeatureManifest manifest) {
        long[][] sweep =
                sweep(
                        n -> {
                            TypedArena<long[]> a = new TypedArena<>(totalOps(n));
                            return p50(n, i -> keep(a.alloc(new long[] {i})));
                        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, null, null);

        TypedArena<long[]> a = new TypedArena<>(totalOps(CANON));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("alloc", p99(CANON, i -> keep(a.alloc(new long[] {i}))));
        // Every timed op here takes the slot the previous one freed, so the
        // reuse path is what is measured rather than the append path.
        TypedArena<long[]> churn = new TypedArena<>(2);
        p99.put("free", p99(CANON, i -> churn.free(churn.alloc(new long[] {i}))));
        manifest.setFeature("typed", d.category(), p99, d.reason());
    }

    // ---------- growable: a new chunk when the active one runs out ----------
    private static void growable(SubMsFeatureManifest manifest) {
        long[][] sweep =
                sweep(
                        n -> {
                            GrowableArena a = new GrowableArena(4096);
                            return p50(n, i -> keep(a.allocate(8, 8)));
                        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, null, null);

        GrowableArena a = new GrowableArena(4096);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("alloc", p99(CANON, i -> keep(a.allocate(8, 8))));
        GrowableArena filled = new GrowableArena(4096);
        for (int i = 0; i < CANON; i++) {
            filled.allocate(8, 8);
        }
        p99.put("reset", p99(1, i -> filled.reset()));
        manifest.setFeature("growable", d.category(), p99, d.reason());
    }

    // ---------- stats: live counters on the alloc path ----------
    private static void stats(SubMsFeatureManifest manifest) {
        long[][] sweep =
                sweep(
                        n -> {
                            StatsArena a = new StatsArena(4096);
                            return p50(n, i -> keep(a.allocate(8, 8)));
                        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, null, null);

        StatsArena a = new StatsArena(4096);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("alloc", p99(CANON, i -> keep(a.allocate(8, 8))));
        p99.put("stats", p99(CANON, i -> keep(a.stats())));
        manifest.setFeature("stats", d.category(), p99, d.reason());
    }

    // ---------- aligned: explicit per-allocation alignment ----------
    private static void aligned(SubMsFeatureManifest manifest) {
        long[][] sweep =
                sweep(
                        n -> {
                            AlignedArena a = new AlignedArena(totalOps(n) * 16 + 4096);
                            return p50(n, i -> keep(a.allocAligned(8, 8)));
                        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, null, null);

        AlignedArena a = new AlignedArena(totalOps(CANON) * 16 + 4096);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("alloc_aligned", p99(CANON, i -> keep(a.allocAligned(8, 8))));
        manifest.setFeature("aligned", d.category(), p99, d.reason());
    }

    // ---------- harness plumbing ----------

    private static long[][] sweep(java.util.function.IntToLongFunction p50At) {
        long[][] rows = new long[SIZES.length][2];
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = p50At.applyAsLong(SIZES[i]);
        }
        return rows;
    }

    /**
     * Allocations a sweep point actually performs: the JIT warmup plus the timed
     * run. The Rust port needs no warmup, so it sizes a fixed-capacity arena to
     * N; sizing to N here overflows it partway through warmup, which is a bench
     * bug, not an arena one.
     */
    private static int totalOps(int n) {
        return warmupFor(n) + n;
    }

    private static int warmupFor(int n) {
        return Math.min(n, 20_000);
    }

    private static long p50(int n, IntConsumer op) {
        return stageStat(run(n, op), true);
    }

    private static long p99(int n, IntConsumer op) {
        return stageStat(run(n, op), false);
    }

    private static SubMsPerfHarness run(int n, IntConsumer op) {
        SubMsPerfHarness h = new SubMsPerfHarness("arena-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", n);
        // Warm to C2 before timing. An unwarmed JIT costs most on the FIRST
        // measured size, which the sweep would read as a cost that falls with N
        // - the opposite of the structural signal, and just as wrong.
        st.warmThenTime(warmupFor(n), n, op);
        return h;
    }

    private static long stageStat(SubMsPerfHarness h, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }

    /** Keep a result observable so the JIT cannot fold the allocation away. */
    private static void keep(Object o) {
        if (o == null) {
            throw new AssertionError("arena returned null");
        }
    }

    private static void keep(int offset) {
        if (offset == Integer.MIN_VALUE) {
            throw new AssertionError("unreachable offset");
        }
    }

    private PerfFeaturesMain() {}
}
