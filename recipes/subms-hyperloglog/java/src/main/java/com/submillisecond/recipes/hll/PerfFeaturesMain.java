package com.submillisecond.recipes.hll;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.hll.features.SparseHyperLogLog;
import com.submillisecond.recipes.hll.features.UnionIntersect;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.Supplier;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Each feature's representative op is swept across three PRECISIONS,
 * {@link SubMsFeatureManifest#classify} DECIDES the category from the shape of
 * that sweep, and the decision plus a measured p99-by-stage is merge-written
 * into {@code ../.subms/features/java.json}.
 *
 * <p>Precision is the sweep axis because it is the only size a HyperLogLog has:
 * {@code p} fixes the register array at {@code 2^p}, and the cardinality being
 * counted changes nothing about the memory touched. {@code add} hashes and
 * writes one register regardless of {@code p}, so it should read flat; anything
 * that folds the whole register array should climb.
 *
 * <p>The register count is capped at {@code 2^18} by the constructor's clamp, so
 * the sweep cannot be pushed an octave higher the way a sketch's width can.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE precision
 * and ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted
 * category is an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.hll.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /** 4096 / 32768 / 262144 registers. 18 is the constructor's ceiling. */
    private static final int[] PRECISIONS = {12, 15, 18};
    private static final int CANON_P = PRECISIONS[PRECISIONS.length - 1];
    /** Keyed ops per measurement. Fixed across the sweep so a slope has one cause. */
    private static final int OPS = 20_000;
    /** Sparse-list lengths. `sparse` is swept over this, not over precision. */
    private static final int[] LIST_LENS = {4_096, 32_768, 262_144};
    private static final int MAX_KEYS = LIST_LENS[LIST_LENS.length - 1];
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

    public static void main(String[] args) throws IOException {
        String[] ks = keys();

        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline is base `add`, the per-op path. NOT base `estimate`: that
        // folds all 2^p registers, so classifying a per-key feature against it
        // would let anything look free.
        HyperLogLog base = new HyperLogLog(CANON_P);
        long baseP50 = keyedP50(head(ks, OPS), base::add);
        System.err.println("base add p50: " + baseP50 + "ns");

        sparse(manifest, baseP50, ks);
        unionIntersect(manifest, baseP50, ks);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- sparse: a linear entry list until it earns the dense array ----------
    private static void sparse(SubMsFeatureManifest manifest, long baseP50, String[] ks) {
        // Swept over SPARSE LIST LENGTH, not over precision. `add` linear-probes
        // the list, so length is the cost driver; precision only sets it
        // indirectly through the `m/4` promotion threshold, and swept that way
        // the curve is a step rather than a slope - at p=12 and p=15 the
        // structure promotes early so BOTH low points measure the dense floor.
        //
        // The list is built to length n OUTSIDE the timed region, and the timed
        // ops are re-adds of keys already in it - a fixed OPS of them at every
        // size, so the op count is constant and the scan length is the only
        // thing varying.
        long[][] sweep = sweepSizes("sparse/add(list-len)", LIST_LENS, n -> {
            SparseHyperLogLog s = new SparseHyperLogLog(CANON_P, n + 1);
            for (int i = 0; i < n; i++) {
                s.add(ks[i]);
            }
            SubMsPerfHarness h = new SubMsPerfHarness("hll-feature", "java");
            SubMsPerfHarness.Stage st = h.stage("op", OPS);
            for (int i = 0; i < OPS; i++) {
                String k = ks[(i * 7919) % n];
                st.time(() -> s.add(k));
            }
            return stat(h, true);
        });
        // PINNED structural when the ratio test cannot carry it, matching the
        // Rust port. `add` linear-probes the sparse list, so it is O(entries)
        // from the source and the sweep is monotonic and strongly rising. What
        // it is not is 32x: a long scan runs far cheaper per element than a
        // short one, so a true O(n) op measures ~23-25x over a 64x span and
        // falls under the classifier's 0.5 guard. Publishing that as hot-path
        // would tell a reader the probe is free at high precision.
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(
                sweep, baseP50, com.submillisecond.perf.SubMsFeatureCategory.STRUCTURAL);

        SparseHyperLogLog s = new SparseHyperLogLog(CANON_P);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("add", keyedP99(head(ks, OPS), s::add));
        p99.put("estimate", bulk(() -> filledSparse(ks), x -> x.estimate(), false));
        manifest.setFeature("sparse", d.category(), p99, d.reason());
    }

    private static SparseHyperLogLog filledSparse(String[] ks) {
        SparseHyperLogLog s = new SparseHyperLogLog(CANON_P);
        for (int i = 0; i < OPS; i++) {
            s.add(ks[i]);
        }
        return s;
    }

    // ---------- union-intersect: pairwise folds over both register arrays ----------
    private static void unionIntersect(SubMsFeatureManifest manifest, long baseP50, String[] ks) {
        // Both HLLs come from the supplier, OUTSIDE the timed region. A union is
        // a pure read of two register arrays, so repeating it does identical work.
        long[][] sweep = sweep("union-intersect/estimateUnion",
                p -> bulk(() -> pair(p, ks), x -> UnionIntersect.estimateUnion(x[0], x[1]), true));
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("union", bulk(() -> pair(CANON_P, ks),
                x -> UnionIntersect.estimateUnion(x[0], x[1]), false));
        p99.put("intersect", bulk(() -> pair(CANON_P, ks),
                x -> UnionIntersect.estimateIntersect(x[0], x[1]), false));
        manifest.setFeature("union-intersect", d.category(), p99, d.reason());
    }

    /**
     * Filled with {@code m} keys, not a fixed count. OCCUPANCY has to be held
     * constant or it, not size, is what the sweep measures: {@code estimate}
     * costs a {@code 2^-r} per register and the zero-register case takes a fast
     * path, so a fixed key set against a growing array leaves most registers
     * zero at p=18 and none at p=12. That reads as a per-register cost falling
     * with size, and it compressed a triple-O(m) op below the classifier guard.
     */
    private static HyperLogLog[] pair(int p, String[] ks) {
        int n = 1 << p;
        HyperLogLog a = new HyperLogLog(p);
        HyperLogLog b = new HyperLogLog(p);
        for (int i = 0; i < n; i++) {
            a.add(ks[i]);
            if (i % 2 == 0) {
                b.add(ks[i]);
            }
        }
        return new HyperLogLog[] {a, b};
    }

    // ---------- harness plumbing ----------

    private static String[] keys() {
        String[] ks = new String[MAX_KEYS];
        for (int i = 0; i < MAX_KEYS; i++) {
            ks[i] = "key-" + i;
        }
        return ks;
    }

    /**
     * Sweeps and PRINTS the curve, indexed by REGISTER COUNT rather than by
     * precision - the classifier reads the size column as a magnitude, and
     * {@code p} is its logarithm.
     */
    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] rows = new long[PRECISIONS.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < PRECISIONS.length; i++) {
            rows[i][0] = 1L << PRECISIONS[i];
            rows[i][1] = at.at(PRECISIONS[i]);
            sb.append('(').append(rows[i][0]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    /** Sweeps over an explicit size column rather than over precision. */
    private static long[][] sweepSizes(String label, int[] sizes, SizedMeasure at) {
        long[][] rows = new long[sizes.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < sizes.length; i++) {
            rows[i][0] = sizes[i];
            rows[i][1] = at.at(sizes[i]);
            sb.append('(').append(sizes[i]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int p);
    }

    private static String[] head(String[] ks, int n) {
        String[] out = new String[n];
        System.arraycopy(ks, 0, out, 0, n);
        return out;
    }

    private static long keyedP50(String[] ks, Consumer<String> op) {
        return stat(keyedRun(ks, op), true);
    }

    private static long keyedP99(String[] ks, Consumer<String> op) {
        return stat(keyedRun(ks, op), false);
    }

    private static SubMsPerfHarness keyedRun(String[] ks, Consumer<String> op) {
        SubMsPerfHarness h = new SubMsPerfHarness("hll-feature", "java");
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

    private static <T> long bulk(Supplier<T> setup, Consumer<T> op, boolean median) {
        T input = setup.get();
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(input);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("hll-feature", "java");
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
