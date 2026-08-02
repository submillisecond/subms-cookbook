package com.submillisecond.recipes.bloom;

import com.submillisecond.perf.SubMsFeatureCategory;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.bloom.features.CountingBloomFilter;
import com.submillisecond.recipes.bloom.features.PartitionedBloomFilter;
import com.submillisecond.recipes.bloom.features.ScalableBloomFilter;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import java.util.function.IntFunction;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Benches each opt-in feature across a size sweep, lets
 * {@link SubMsFeatureManifest#classify} DECIDE the category from the shape of
 * that sweep, and merge-writes the decision into
 * {@code ../.subms/features/java.json} - preserving any field already there.
 *
 * <p>The category is measured, not asserted: a p99 that stays flat as the
 * structure grows is hot-path, one that scales with size is structural, and a
 * feature with no hot-path workload is auxiliary.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.bloom.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];

    public static void main(String[] args) throws IOException {
        String[] ks = keys(CANON);

        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // ---------- counting: adds remove() over 4-bit counters ----------
        IntFunction<CountingBloomFilter> countingFill = n -> {
            CountingBloomFilter c = new CountingBloomFilter(n);
            for (int i = 0; i < n; i++) c.add(ks[i]);
            return c;
        };
        long[][] countingSweep = sweep(n -> probeP99(n, () -> countingFill.apply(n), (c, k) -> c.mightContain(k), ks));
        SubMsFeatureManifest.Decision counting = SubMsFeatureManifest.classify(countingSweep, null, null);
        Map<String, Long> countingP99 = new LinkedHashMap<>();
        countingP99.put("contains", countingSweep[countingSweep.length - 1][1]);
        countingP99.put("add", mutateP99(CANON, () -> new CountingBloomFilter(CANON), (c, k) -> c.add(k), ks));
        countingP99.put("remove", mutateP99(CANON, () -> countingFill.apply(CANON), (c, k) -> c.remove(k), ks));
        manifest.setFeature("counting", counting.category(), countingP99, counting.reason());

        // ---------- scalable: layers grow as cardinality does ----------
        IntFunction<ScalableBloomFilter> scalableFill = n -> {
            ScalableBloomFilter s = new ScalableBloomFilter(1_000);
            for (int i = 0; i < n; i++) s.add(ks[i]);
            return s;
        };
        long[][] scalableSweep = sweep(n -> probeP99(n, () -> scalableFill.apply(n), (s, k) -> s.mightContain(k), ks));
        SubMsFeatureManifest.Decision scalable = SubMsFeatureManifest.classify(scalableSweep, null, null);
        Map<String, Long> scalableP99 = new LinkedHashMap<>();
        scalableP99.put("contains", scalableSweep[scalableSweep.length - 1][1]);
        scalableP99.put("add", mutateP99(CANON, () -> new ScalableBloomFilter(1_000), (s, k) -> s.add(k), ks));
        manifest.setFeature("scalable", scalable.category(), scalableP99, scalable.reason());

        // ---------- partitioned: k independent slices ----------
        IntFunction<PartitionedBloomFilter> partFill = n -> {
            PartitionedBloomFilter p = new PartitionedBloomFilter(n);
            for (int i = 0; i < n; i++) p.add(ks[i]);
            return p;
        };
        long[][] partSweep = sweep(n -> probeP99(n, () -> partFill.apply(n), (p, k) -> p.mightContain(k), ks));
        SubMsFeatureManifest.Decision partitioned = SubMsFeatureManifest.classify(partSweep, null, null);
        Map<String, Long> partP99 = new LinkedHashMap<>();
        partP99.put("contains", partSweep[partSweep.length - 1][1]);
        partP99.put("add", mutateP99(CANON, () -> new PartitionedBloomFilter(CANON), (p, k) -> p.add(k), ks));
        manifest.setFeature("partitioned", partitioned.category(), partP99, partitioned.reason());

        manifest.save(path);
        System.out.println(manifest.toJson());
    }

    private static String[] keys(int n) {
        String[] ks = new String[n];
        for (int i = 0; i < n; i++) ks[i] = "key-" + i;
        return ks;
    }

    private static long[][] sweep(java.util.function.IntToLongFunction p99At) {
        long[][] rows = new long[SIZES.length][2];
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = p99At.applyAsLong(SIZES[i]);
        }
        return rows;
    }

    /** p99 (ns) of a read probe over {@code size} keys against a pre-filled filter. */
    private static <T> long probeP99(int size, java.util.function.Supplier<T> build, BiConsumer<T, String> probe, String[] ks) {
        T f = build.get();
        SubMsPerfHarness h = new SubMsPerfHarness("bloom-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("probe", size);
        st.warmThenTime(Math.min(size, 20_000), size, (int i) -> probe.accept(f, ks[i]));
        return stageP99(h, "probe");
    }

    /** p99 (ns) of a mutating op over {@code size} keys against a fresh filter. */
    private static <T> long mutateP99(int size, java.util.function.Supplier<T> make, BiConsumer<T, String> op, String[] ks) {
        T f = make.get();
        SubMsPerfHarness h = new SubMsPerfHarness("bloom-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", size);
        st.warmThenTime(Math.min(size, 20_000), size, (int i) -> op.accept(f, ks[i]));
        return stageP99(h, "op");
    }

    private static long stageP99(SubMsPerfHarness h, String name) {
        return com.submillisecond.perf.SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals(name))
                .findFirst()
                .map(s -> s.p99Ns())
                .orElse(0L);
    }

    private PerfFeaturesMain() {}
}
