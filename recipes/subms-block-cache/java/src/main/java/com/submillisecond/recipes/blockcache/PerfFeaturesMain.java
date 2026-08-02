package com.submillisecond.recipes.blockcache;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.blockcache.features.ArcCache;
import com.submillisecond.recipes.blockcache.features.MetricsCache;
import com.submillisecond.recipes.blockcache.features.ShardedCache;
import com.submillisecond.recipes.blockcache.features.TinyLfuCache;
import com.submillisecond.recipes.blockcache.features.WeightedCache;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.IntConsumer;
import java.util.Random;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Sweeps each feature (arc, tinylfu, weighted, concurrent-shards, metrics)
 * across three cache capacities, lets {@link SubMsFeatureManifest#classify}
 * DECIDE the category from the shape of that sweep, and merge-writes the
 * decision into {@code ../.subms/features/java.json}.
 *
 * <p>A cache's "size" is its capacity, so the sweep fills to N and times the
 * lookup path there. A per-op cost that holds steady as N grows is hot-path; one
 * that climbs with N is structural. That is the claim worth measuring for a
 * cache: an eviction policy that quietly walks the resident set does not stay
 * sub-millisecond as the cache grows, and only a sweep catches it.
 *
 * <p>The sweep classifies on p50, and the BASELINE is a p50 too. Mixing them -
 * a p50 sweep point against a p99 baseline - compares two different statistics,
 * and since the p50 sits under the p99 almost by construction, every feature
 * would read as a non-effect.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE capacity
 * and ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted
 * category is an opinion the bench cannot contradict; a sweep measures it.
 *
 * <p>clock-sweep is not a separate variant: it IS the base cache, so its lookup
 * is the baseline every feature is classified against.
 *
 * <p>These p99 figures describe THIS machine. They are published only when the
 * manifest is stamped {@code p99_source: fleet}; a local run leaves the category,
 * which is machine independent, and no published number.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.blockcache.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    /** Cache capacities the sweep walks. Mirrors SIZES in the Rust port. */
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];
    private static final long SEED = 0L;

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: base clock-sweep lookup at the canonical capacity. A
        // variant landing at or under this costs nothing on the hot path.
        BlockCache<Integer, Long> base = new BlockCache<>(CANON);
        fill(CANON, k -> base.put(k, (long) k));
        long baseP50 = getHitP50(CANON, k -> base.get(k) != null);

        arc(manifest, baseP50);
        tinylfu(manifest, baseP50);
        weighted(manifest, baseP50);
        shards(manifest, baseP50);
        metrics(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- arc: adaptive replacement, recency + frequency lists ----------
    private static void arc(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep(n -> {
            ArcCache<Integer, Long> c = new ArcCache<>(n);
            fill(n, k -> c.put(k, (long) k));
            return getHitP50(n, k -> c.get(k) != null);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        ArcCache<Integer, Long> c = new ArcCache<>(CANON);
        fill(CANON, k -> c.put(k, (long) k));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get_hit", getHitP99(CANON, k -> c.get(k) != null));
        p99.put("put", putP99(CANON, k -> c.put(k, (long) k)));
        manifest.setFeature("arc", d.category(), p99, d.reason());
    }

    // ---------- tinylfu: frequency-sketch admission ----------
    private static void tinylfu(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep(n -> {
            TinyLfuCache<Integer, Long> c = new TinyLfuCache<>(n);
            fill(n, k -> c.put(k, (long) k));
            return getHitP50(n, k -> c.get(k) != null);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        TinyLfuCache<Integer, Long> c = new TinyLfuCache<>(CANON);
        fill(CANON, k -> c.put(k, (long) k));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get_hit", getHitP99(CANON, k -> c.get(k) != null));
        p99.put("put", putP99(CANON, k -> c.put(k, (long) k)));
        manifest.setFeature("tinylfu", d.category(), p99, d.reason());
    }

    // ---------- weighted: a byte budget rather than a slot count ----------
    private static void weighted(SubMsFeatureManifest manifest, long baseP50) {
        // 1 byte per entry so capacityBytes == slot capacity; eviction behaves
        // like the base cache, which isolates the weight bookkeeping itself.
        long[][] sweep = sweep(n -> {
            WeightedCache<Integer, Long> c = new WeightedCache<>(n, v -> 1);
            fill(n, k -> c.put(k, (long) k));
            return getHitP50(n, k -> c.get(k) != null);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        WeightedCache<Integer, Long> c = new WeightedCache<>(CANON, v -> 1);
        fill(CANON, k -> c.put(k, (long) k));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get_hit", getHitP99(CANON, k -> c.get(k) != null));
        p99.put("put", putP99(CANON, k -> c.put(k, (long) k)));
        manifest.setFeature("weighted", d.category(), p99, d.reason());
    }

    // ---------- concurrent-shards: measured single-threaded ----------
    // Uncontended on purpose. This isolates the sharding INDIRECTION from the
    // contention it exists to relieve; a multi-threaded number here would say
    // more about the thread count than about the feature.
    private static void shards(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep(n -> {
            ShardedCache<Integer, Long> c = new ShardedCache<>(n, 16);
            fill(n, k -> c.put(k, (long) k));
            return getHitP50(n, k -> c.get(k) != null);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        ShardedCache<Integer, Long> c = new ShardedCache<>(CANON, 16);
        fill(CANON, k -> c.put(k, (long) k));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get_hit", getHitP99(CANON, k -> c.get(k) != null));
        p99.put("put", putP99(CANON, k -> c.put(k, (long) k)));
        manifest.setFeature("concurrent-shards", d.category(), p99, d.reason());
    }

    // ---------- metrics: hit/miss counters on the lookup path ----------
    private static void metrics(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep(n -> {
            MetricsCache<Integer, Long> c = new MetricsCache<>(n);
            fill(n, k -> c.put(k, (long) k));
            return getHitP50(n, k -> c.get(k) != null);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        MetricsCache<Integer, Long> c = new MetricsCache<>(CANON);
        fill(CANON, k -> c.put(k, (long) k));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get_hit", getHitP99(CANON, k -> c.get(k) != null));
        p99.put("put", putP99(CANON, k -> c.put(k, (long) k)));
        manifest.setFeature("metrics", d.category(), p99, d.reason());
    }

    // ---------- harness plumbing ----------

    private static long[][] sweep(IntUnaryOperatorLong p50At) {
        long[][] rows = new long[SIZES.length][2];
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = p50At.applyAsLong(SIZES[i]);
        }
        return rows;
    }

    /** A size-indexed measurement. IntUnaryOperator returns int, not long. */
    @FunctionalInterface
    private interface IntUnaryOperatorLong {
        long applyAsLong(int n);
    }

    private static void fill(int n, IntConsumer put) {
        for (int k = 0; k < n; k++) {
            put.accept(k);
        }
    }

    /** p50 (ns) of n reads of keys drawn from the resident set 0..n. */
    private static long getHitP50(int n, java.util.function.IntPredicate get) {
        return stat(getHitRun(n, get), true);
    }

    /** p99 (ns) of the same read workload. */
    private static long getHitP99(int n, java.util.function.IntPredicate get) {
        return stat(getHitRun(n, get), false);
    }

    private static SubMsPerfHarness getHitRun(int n, java.util.function.IntPredicate get) {
        // Seeded so the key sequence is identical across runs and sizes; a
        // different draw order would move the p50 for reasons unrelated to size.
        Random rng = new Random(SEED);
        SubMsPerfHarness h = new SubMsPerfHarness("block-cache-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", n);
        // Warm to C2 first. An unwarmed JIT costs most on the FIRST measured
        // size, which the sweep would read as a cost that FALLS with N - the
        // opposite of the structural signal, and just as wrong.
        int warm = Math.min(n, 20_000);
        for (int i = 0; i < warm; i++) {
            get.test(rng.nextInt(n));
        }
        for (int i = 0; i < n; i++) {
            int key = rng.nextInt(n);
            st.time(() -> get.test(key));
        }
        return h;
    }

    /**
     * p99 (ns) of n inserts of FRESH keys against a full cache, so every put
     * drives an eviction - the path a capacity claim actually rests on.
     */
    private static long putP99(int n, IntConsumer put) {
        SubMsPerfHarness h = new SubMsPerfHarness("block-cache-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", n);
        for (int i = 0; i < n; i++) {
            int key = n + i;
            st.time(() -> put.accept(key));
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
