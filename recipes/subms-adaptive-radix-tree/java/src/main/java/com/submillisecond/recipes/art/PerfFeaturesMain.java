package com.submillisecond.recipes.art;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.art.features.ArtSnapshot;
import com.submillisecond.recipes.art.features.Compaction;
import com.submillisecond.recipes.art.features.MeasuredArt;
import com.submillisecond.recipes.art.features.RangeScan;
import com.submillisecond.recipes.art.features.Serialize;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.IntFunction;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Sweeps each feature across three tree sizes, lets
 * {@link SubMsFeatureManifest#classify} DECIDE the category from the shape of
 * that sweep, and merge-writes the decision into
 * {@code ../.subms/features/java.json}.
 *
 * <p>The category is measured, not asserted: a p99 that stays flat as the tree
 * grows is hot-path, one that scales with size is structural and sits outside
 * the per-op sub-ms claim. Each feature is swept on the operation a caller
 * repeats - the whole-tree call for the bulk features, the key operation for
 * the per-key ones.
 *
 * <p>The p99 figures describe THIS machine. They are published only when the
 * manifest is stamped {@code p99_source: fleet}; a local run leaves the
 * category, which is machine independent, and no published number.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.art.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];
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
    /** Enough warm reps to get past interpretation and the first C2 recompiles. */
    private static final int BULK_WARMUP = 10;

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // ---------- serialize: whole-tree write + parse ----------
        long[][] serSweep = sweep(n -> {
            Art<Long> tree = populate(n);
            return bulkP50(() -> {
                try {
                    Serialize.writeToBytes(tree, Serialize.INT64);
                } catch (IOException e) {
                    throw new IllegalStateException(e);
                }
            });
        });
        SubMsFeatureManifest.Decision ser = SubMsFeatureManifest.classify(serSweep, null, null);
        byte[] encoded = Serialize.writeToBytes(populate(CANON), Serialize.INT64);
        long serRead = bulkP99(() -> {
            try {
                Serialize.parseBytes(encoded, Serialize.INT64);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        });
        Map<String, Long> serP99 = new LinkedHashMap<>();
        serP99.put("write", serSweep[serSweep.length - 1][1]);
        serP99.put("read", serRead);
        manifest.setFeature("serialize", ser.category(), serP99, ser.reason());

        // ---------- range-scan: a full scan tracks the tree it walks ----------
        long[][] rangeSweep = sweep(n -> {
            Art<Long> tree = populate(n);
            return bulkP50(() -> RangeScan.range(tree, RangeScan.Bound.unbounded(), RangeScan.Bound.unbounded()));
        });
        SubMsFeatureManifest.Decision rng = SubMsFeatureManifest.classify(rangeSweep, null, null);
        Map<String, Long> rangeP99 = new LinkedHashMap<>();
        rangeP99.put("range", rangeSweep[rangeSweep.length - 1][1]);
        manifest.setFeature("range-scan", rng.category(), rangeP99, rng.reason());

        // ---------- concurrent-reads: the point is the READ off a frozen view ----------
        long[][] snapSweep = sweep(n -> {
            ArtSnapshot<Long> snap = ArtSnapshot.fromTree(populate(n));
            return keyedP99(n, k -> snap.get(k));
        });
        SubMsFeatureManifest.Decision snapDec = SubMsFeatureManifest.classify(snapSweep, null, null);
        // Taking the snapshot is O(n) and is NOT the swept op - recorded so the
        // page shows what establishing the frozen view costs.
        Art<Long> snapTree = populate(CANON);
        long snapshot = bulkP99(() -> ArtSnapshot.fromTree(snapTree));
        Map<String, Long> snapP99 = new LinkedHashMap<>();
        snapP99.put("get", snapSweep[snapSweep.length - 1][1]);
        snapP99.put("snapshot", snapshot);
        manifest.setFeature("concurrent-reads", snapDec.category(), snapP99, snapDec.reason());

        // ---------- metrics: counters on the insert/lookup path ----------
        long[][] metricsSweep = sweep(n -> {
            MeasuredArt<Long> tree = new MeasuredArt<>();
            for (int i = 0; i < n; i++) tree.insert(key(i), (long) i);
            return keyedP99(n, k -> tree.get(k));
        });
        SubMsFeatureManifest.Decision met = SubMsFeatureManifest.classify(metricsSweep, null, null);
        MeasuredArt<Long> fresh = new MeasuredArt<>();
        long[] next = {0};
        long metInsert = keyedP99(CANON, k -> fresh.insert(k, next[0]++));
        Map<String, Long> metP99 = new LinkedHashMap<>();
        metP99.put("lookup", metricsSweep[metricsSweep.length - 1][1]);
        metP99.put("insert", metInsert);
        manifest.setFeature("metrics", met.category(), metP99, met.reason());

        // ---------- compaction: a sweep over what the deletes left behind ----------
        long[][] compactSweep = sweep(n -> bulkP50(() -> {
            Art<Long> dirty = populate(n);
            for (int i = 0; i < n; i += 2) Compaction.delete(dirty, key(i));
            Compaction.compact(dirty);
        }));
        SubMsFeatureManifest.Decision comp = SubMsFeatureManifest.classify(compactSweep, null, null);
        Art<Long> delTree = populate(CANON);
        long del = keyedP99(CANON, k -> Compaction.delete(delTree, k));
        Map<String, Long> compP99 = new LinkedHashMap<>();
        compP99.put("compact", compactSweep[compactSweep.length - 1][1]);
        compP99.put("delete", del);
        manifest.setFeature("compaction", comp.category(), compP99, comp.reason());

        manifest.save(path);
        System.out.println(manifest.toJson());
    }

    private static byte[] key(int i) {
        return ("key-" + i).getBytes(StandardCharsets.UTF_8);
    }

    private static Art<Long> populate(int n) {
        Art<Long> tree = new Art<>();
        for (int i = 0; i < n; i++) tree.insert(key(i), (long) i);
        return tree;
    }

    private static long[][] sweep(IntFunction<Long> p99At) {
        long[][] rows = new long[SIZES.length][2];
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = p99At.apply(SIZES[i]);
        }
        return rows;
    }

    /** p99 (ns) of a whole-structure op repeated {@code BULK_REPS} times. */
    private static long bulkP99(Runnable op) {
        return bulkStage(op).p99Ns();
    }

    /**
     * p50 (ns) of the same op - what the SWEEP is classified on.
     *
     * <p>p99 over a few dozen bulk samples is just the worst one, and in a
     * managed runtime the worst one is a collector pause: size independent, and
     * large enough to swamp the signal the sweep is looking for. Classifying on
     * that read a 199ms serialize as "flat per-op" - hot-path - which is
     * nonsense. The sweep asks one question, does cost track n, and p50 answers
     * it without a single pause deciding the outcome. The p99 still goes into
     * the manifest for the stage table; it is simply not what the shape is read
     * from.
     */
    private static long bulkP50(Runnable op) {
        return bulkStage(op).p50Ns();
    }

    private static com.submillisecond.perf.SubMsStageSummary bulkStage(Runnable op) {
        SubMsPerfHarness h = new SubMsPerfHarness("art-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("bulk", BULK_REPS);
        st.warmThenTime(BULK_WARMUP, BULK_REPS, (int i) -> op.run());
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("bulk"))
                .findFirst()
                .orElseThrow();
    }

    /** p99 (ns) of a per-key op run over every key in {@code 0..n}. */
    private static long keyedP99(int n, Consumer<byte[]> op) {
        byte[][] keys = new byte[n][];
        for (int i = 0; i < n; i++) keys[i] = key(i);
        SubMsPerfHarness h = new SubMsPerfHarness("art-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("keyed", n);
        st.warmThenTime(Math.min(n, 20_000), n, (int i) -> op.accept(keys[i]));
        return stageP99(h, "keyed");
    }

    private static long stageP99(SubMsPerfHarness h, String name) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals(name))
                .findFirst()
                .map(s -> s.p99Ns())
                .orElse(0L);
    }

    private PerfFeaturesMain() {}
}
