package com.submillisecond.recipes.cuckoo;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureCategory;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.cuckoo.features.CompressedCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.CuckooSnapshot;
import com.submillisecond.recipes.cuckoo.features.DynamicCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Sweeps each feature (variable-fingerprint, dynamic, concurrent-reads,
 * compressed-buckets) across three filter sizes, lets
 * {@link SubMsFeatureManifest#classify} DECIDE the category from the shape of
 * that sweep, and merge-writes the decision into
 * {@code ../.subms/features/java.json}.
 *
 * <p>A filter's "size" is how many keys it is holding, so the sweep fills to N
 * and times the lookup path there. A per-op cost that holds steady as N grows is
 * hot-path; one that climbs with N is structural.
 *
 * <p>The sweep classifies on p50, and the BASELINE is a p50 too. Mixing them - a
 * p50 sweep point against a p99 baseline - compares different statistics, and
 * the p50 sits under the p99 almost by construction, so every feature would read
 * as a non-effect.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it.
 *
 * <p>These p99 figures describe THIS machine. They are published only when the
 * manifest is stamped {@code p99_source: fleet}; a local run leaves the category,
 * which is machine independent for the SCALING verdict, and no published number.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.cuckoo.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /** Key counts the sweep walks. Mirrors SIZES in the Rust port. */
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];
    /** Timed repeats for the one-shot snapshot capture. */
    private static final int SNAPSHOT_REPS = 32;
    /** Untimed captures first - see the concurrent-reads block. */
    private static final int SNAPSHOT_WARM = 16;

    public static void main(String[] args) throws IOException {
        String[] canonKeys = keys(CANON);

        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline. A variant landing at or under it costs nothing on the
        // hot path, and classify says so rather than defaulting to hot-path.
        CuckooFilter base = new CuckooFilter(CANON);
        for (String k : canonKeys) {
            base.insert(k);
        }
        long baseP50 = p50(canonKeys, k -> base.contains(k));

        variableFingerprint(manifest, baseP50, canonKeys);
        dynamic(manifest, baseP50, canonKeys);
        concurrentReads(manifest, canonKeys);
        compressedBuckets(manifest, baseP50, canonKeys);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- variable-fingerprint: wider tag, lower FPR ----------
    private static void variableFingerprint(
            SubMsFeatureManifest manifest, long baseP50, String[] canonKeys) {
        long[][] sweep = sweep(n -> {
            String[] ks = keys(n);
            VariableFpCuckooFilter f = new VariableFpCuckooFilter(n, FingerprintWidth.SIXTEEN);
            for (String k : ks) {
                f.insert(k);
            }
            return p50(ks, k -> f.contains(k));
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        VariableFpCuckooFilter f = new VariableFpCuckooFilter(CANON, FingerprintWidth.SIXTEEN);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("insert", p99(canonKeys, f::insert));
        p99.put("lookup", p99(canonKeys, f::contains));
        p99.put("delete", p99(canonKeys, f::delete));
        manifest.setFeature("variable-fingerprint", d.category(), p99, d.reason());
    }

    // ---------- dynamic: grows rather than refusing at load factor ----------
    private static void dynamic(SubMsFeatureManifest manifest, long baseP50, String[] canonKeys) {
        long[][] sweep = sweep(n -> {
            String[] ks = keys(n);
            DynamicCuckooFilter f = new DynamicCuckooFilter(n);
            for (String k : ks) {
                f.insert(k);
            }
            return p50(ks, k -> f.contains(k));
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        DynamicCuckooFilter f = new DynamicCuckooFilter(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("insert", p99(canonKeys, f::insert));
        p99.put("lookup", p99(canonKeys, f::contains));
        p99.put("delete", p99(canonKeys, f::delete));
        manifest.setFeature("dynamic", d.category(), p99, d.reason());
    }

    // ---------- concurrent-reads: a frozen snapshot readers share ----------
    private static void concurrentReads(SubMsFeatureManifest manifest, String[] canonKeys) {
        CuckooFilter src = new CuckooFilter(CANON);
        for (String k : canonKeys) {
            src.insert(k);
        }
        for (int i = 0; i < SNAPSHOT_WARM; i++) {
            CuckooSnapshot.capture(src);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("cuckoo-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", SNAPSHOT_REPS);
        for (int i = 0; i < SNAPSHOT_REPS; i++) {
            st.time(() -> CuckooSnapshot.capture(src));
        }
        long snap99 = stat(h, false);
        CuckooSnapshot snap = CuckooSnapshot.capture(src);

        // PINNED structural, not measured - matching the Rust port, and for the
        // same reason. CuckooSnapshot.capture copies the whole bucket array, so
        // it is unambiguously O(N) from the source, but the sweep cannot show it
        // on a dev box: even with warmup discarded the smallest size measures
        // slower than 8x its size does, a non-monotonic curve whose min/max
        // ratio reads ~2x over a 64x size range, so the scaling test calls it
        // flat and an O(N) copy classifies hot-path. Recording that would be a
        // false claim about the one op here that genuinely is not per-op.
        // perfReason says it was overridden rather than measured.
        SubMsFeatureManifest.Decision d =
                SubMsFeatureManifest.classify(
                        new long[][] {{CANON, snap99}}, null, SubMsFeatureCategory.STRUCTURAL);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("snapshot", snap99);
        p99.put("lookup_on_snapshot", p99(canonKeys, snap::contains));
        manifest.setFeature("concurrent-reads", d.category(), p99, d.reason());
    }

    // ---------- compressed-buckets: tighter memory per bucket ----------
    private static void compressedBuckets(
            SubMsFeatureManifest manifest, long baseP50, String[] canonKeys) {
        long[][] sweep = sweep(n -> {
            String[] ks = keys(n);
            CompressedCuckooFilter f = new CompressedCuckooFilter(n);
            for (String k : ks) {
                f.insert(k);
            }
            return p50(ks, k -> f.contains(k));
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        CompressedCuckooFilter f = new CompressedCuckooFilter(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("insert", p99(canonKeys, f::insert));
        p99.put("lookup", p99(canonKeys, f::contains));
        p99.put("delete", p99(canonKeys, f::delete));
        manifest.setFeature("compressed-buckets", d.category(), p99, d.reason());
    }

    // ---------- harness plumbing ----------

    private static String[] keys(int n) {
        String[] ks = new String[n];
        for (int i = 0; i < n; i++) {
            ks[i] = "key-" + i;
        }
        return ks;
    }

    private static long[][] sweep(SizedMeasure p50At) {
        long[][] rows = new long[SIZES.length][2];
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = p50At.at(SIZES[i]);
        }
        return rows;
    }

    /** A size-indexed measurement; IntUnaryOperator returns int, not long. */
    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n);
    }

    private static long p50(String[] ks, Consumer<String> op) {
        return stat(run(ks, op), true);
    }

    private static long p99(String[] ks, Consumer<String> op) {
        return stat(run(ks, op), false);
    }

    private static SubMsPerfHarness run(String[] ks, Consumer<String> op) {
        SubMsPerfHarness h = new SubMsPerfHarness("cuckoo-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", ks.length);
        // Warm to C2 first. An unwarmed JIT costs most on the FIRST measured
        // size, which the sweep reads as a cost that FALLS with N - the opposite
        // of the structural signal, and just as wrong.
        int warm = Math.min(ks.length, 20_000);
        for (int i = 0; i < warm; i++) {
            op.accept(ks[i]);
        }
        for (String k : ks) {
            st.time(() -> op.accept(k));
        }
        return h;
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
