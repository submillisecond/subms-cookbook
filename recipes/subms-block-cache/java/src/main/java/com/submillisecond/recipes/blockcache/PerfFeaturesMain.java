package com.submillisecond.recipes.blockcache;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.blockcache.features.ArcCache;
import com.submillisecond.recipes.blockcache.features.MetricsCache;
import com.submillisecond.recipes.blockcache.features.ShardedCache;
import com.submillisecond.recipes.blockcache.features.TinyLfuCache;
import com.submillisecond.recipes.blockcache.features.WeightedCache;

import java.io.IOException;
import java.util.Random;
import java.util.function.IntPredicate;
import java.util.function.IntConsumer;
import java.util.function.Supplier;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Each cache is pre-populated to its capacity, then two stages run: {@code
 * *_get_hit} reads keys drawn from the resident set, and {@code *_put}
 * inserts fresh keys against a full cache so every put drives an eviction.
 * One stage block per variant - base_get_hit, base_put, arc_get_hit, etc. -
 * with the SAME stage names as the Rust bench so the cookbook FeaturePicker
 * columns line up across languages. JSON contract goes to stdout.
 *
 * <p>The Rust bench gates each feature behind a Cargo feature; the Java
 * artefact ships every feature in one jar, so all variants always emit.
 * clock-sweep is not a separate variant: it IS the base cache, so the
 * base_* stages already measure it.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.blockcache.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final long SEED = 0L;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("block-cache-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Long.toString(SEED));
        h.meta("subms.recipe.slug", "subms-block-cache");
        h.meta("subms.recipe.category", "memory");

        // ---------- base (clock-sweep) ----------
        h.meta("subms.workload.feature", "base");
        BlockCache<Integer, Integer> base = new BlockCache<>(ENTRIES);
        for (int k = 0; k < ENTRIES; k++) base.put(k, k);
        benchGetHit(h, "base_get_hit", key -> base.get(key) != null);
        benchPut(h, "base_put", () -> {
            BlockCache<Integer, Integer> c = new BlockCache<>(ENTRIES);
            for (int k = 0; k < ENTRIES; k++) c.put(k, k);
            return key -> c.put(key, key);
        });

        // ---------- arc ----------
        h.meta("subms.workload.feature", "arc");
        ArcCache<Integer, Integer> arc = new ArcCache<>(ENTRIES);
        for (int k = 0; k < ENTRIES; k++) arc.put(k, k);
        benchGetHit(h, "arc_get_hit", key -> arc.get(key) != null);
        benchPut(h, "arc_put", () -> {
            ArcCache<Integer, Integer> c = new ArcCache<>(ENTRIES);
            for (int k = 0; k < ENTRIES; k++) c.put(k, k);
            return key -> c.put(key, key);
        });

        // ---------- tinylfu ----------
        h.meta("subms.workload.feature", "tinylfu");
        TinyLfuCache<Integer, Integer> tinylfu = new TinyLfuCache<>(ENTRIES);
        for (int k = 0; k < ENTRIES; k++) tinylfu.put(k, k);
        benchGetHit(h, "tinylfu_get_hit", key -> tinylfu.get(key) != null);
        benchPut(h, "tinylfu_put", () -> {
            TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(ENTRIES);
            for (int k = 0; k < ENTRIES; k++) c.put(k, k);
            return key -> c.put(key, key);
        });

        // ---------- weighted ----------
        h.meta("subms.workload.feature", "weighted");
        // 1 byte per entry so capacity_bytes == slot capacity; eviction
        // behaves like the base cache on puts.
        WeightedCache<Integer, Integer> weighted =
            new WeightedCache<>(ENTRIES, v -> 1);
        for (int k = 0; k < ENTRIES; k++) weighted.put(k, k);
        benchGetHit(h, "weighted_get_hit", key -> weighted.get(key) != null);
        benchPut(h, "weighted_put", () -> {
            WeightedCache<Integer, Integer> c = new WeightedCache<>(ENTRIES, v -> 1);
            for (int k = 0; k < ENTRIES; k++) c.put(k, k);
            return key -> c.put(key, key);
        });

        // ---------- concurrent-shards (single-threaded) ----------
        h.meta("subms.workload.feature", "concurrent-shards");
        ShardedCache<Integer, Integer> sharded = new ShardedCache<>(ENTRIES, 16);
        for (int k = 0; k < ENTRIES; k++) sharded.put(k, k);
        benchGetHit(h, "concurrent_shards_get_hit", key -> sharded.get(key) != null);
        benchPut(h, "concurrent_shards_put", () -> {
            ShardedCache<Integer, Integer> c = new ShardedCache<>(ENTRIES, 16);
            for (int k = 0; k < ENTRIES; k++) c.put(k, k);
            return key -> c.put(key, key);
        });

        // ---------- metrics ----------
        h.meta("subms.workload.feature", "metrics");
        MetricsCache<Integer, Integer> metrics = new MetricsCache<>(ENTRIES);
        for (int k = 0; k < ENTRIES; k++) metrics.put(k, k);
        benchGetHit(h, "metrics_get_hit", key -> metrics.get(key) != null);
        benchPut(h, "metrics_put", () -> {
            MetricsCache<Integer, Integer> c = new MetricsCache<>(ENTRIES);
            for (int k = 0; k < ENTRIES; k++) c.put(k, k);
            return key -> c.put(key, key);
        });

        h.writeJson(System.out);
    }

    /** Time ENTRIES reads of keys drawn from the resident set {@code 0..ENTRIES}.
     *  get reorders LRU but never evicts, so warming on the measured cache
     *  leaves every key resident for the timed hits. */
    private static void benchGetHit(SubMsPerfHarness h, String name, IntPredicate get) {
        Random rng = new Random(SEED);
        int[] keys = new int[ENTRIES];
        for (int i = 0; i < ENTRIES; i++) keys[i] = rng.nextInt(ENTRIES);
        SubMsPerfHarness.Stage stage = h.stage(name, ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        stage.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES,
            (int i) -> get.test(keys[i % keys.length]));
    }

    /** Time ENTRIES inserts of fresh keys against a full cache; each put
     *  drives an eviction. Warming would dirty the measured cache's resident
     *  set (re-inserting warmed keys turns later puts into no-evict updates),
     *  so warm on a throwaway cache from {@code freshTarget} and measure on a
     *  second one built the same way. */
    private static void benchPut(SubMsPerfHarness h, String name, Supplier<IntConsumer> freshTarget) {
        IntConsumer warmPut = freshTarget.get();
        int warm = Math.min(ENTRIES, 20_000);
        for (int i = 0; i < warm; i++) warmPut.accept(ENTRIES + i);

        IntConsumer put = freshTarget.get();
        SubMsPerfHarness.Stage stage = h.stage(name, ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            int key = ENTRIES + i;
            stage.time(() -> put.accept(key));
        }
    }
}
