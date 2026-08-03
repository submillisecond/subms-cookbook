package com.submillisecond.recipes.treap;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.treap.features.PersistentTreap;
import com.submillisecond.recipes.treap.features.RangeQuery;
import com.submillisecond.recipes.treap.features.SplittableTreap;
import com.submillisecond.recipes.treap.features.TreapSnapshot;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.IntConsumer;
import java.util.function.Supplier;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Each feature's representative op is swept across three tree sizes,
 * {@link SubMsFeatureManifest#classify} DECIDES the category from the shape of
 * that sweep, and the decision plus a measured p99-by-stage is merge-written
 * into {@code ../.subms/features/java.json}.
 *
 * <p>A treap's ops are O(log n) expected, so on a 64x size sweep a per-op
 * feature should rise by well under 2x - flat, by the classifier's reading.
 * Anything that walks the tree instead of descending it rises with n, and that
 * is the line the sweep is here to draw. {@code split} looks like the former and
 * is the latter.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.treap.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int[] SIZES = {4_096, 32_768, 262_144};
    private static final int CANON = SIZES[SIZES.length - 1];
    private static final long SEED = 0L;
    /** Keyed ops per measurement. Fixed across the sweep so a slope has one cause. */
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
    private static final long RANGE_TAKE = 64L;
    private static final long KEY_SPACE = 1_000_000_007L;

    /**
     * Key-space width that yields about {@code RANGE_TAKE} hits in a tree of
     * {@code n} keys. A fixed width would return 64x more rows at the top of the
     * sweep and the classifier would be reading the answer size, not the query.
     */
    private static long rangeWidth(int n) {
        return (KEY_SPACE / n) * RANGE_TAKE;
    }

    /** Scattered rather than ascending, so a descent cannot be predicted away. */
    private static long keyAt(int i) {
        return (i * 2_654_435_761L) % KEY_SPACE;
    }

    private static Treap<Long, Long> build(int n) {
        Treap<Long, Long> t = new Treap<>(SEED);
        for (int i = 0; i < n; i++) {
            t.insert(keyAt(i), (long) i);
        }
        return t;
    }

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: a base-treap lookup at the canonical size. A feature
        // landing at or under this costs nothing on the read path.
        Treap<Long, Long> base = build(CANON);
        long baseP50 = keyed(CANON, i -> base.get(keyAt(i)), true);
        System.err.println("base get p50: " + baseP50 + "ns");

        rangeQuery(manifest, baseP50);
        persistent(manifest, baseP50);
        mergeSplit(manifest, baseP50);
        concurrentReads(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- range-query: an in-order walk between two bounds ----------
    private static void rangeQuery(SubMsFeatureManifest manifest, long baseP50) {
        // The window is sized to yield a constant number of rows at every sweep
        // point; a fixed key-space width would make the answer 64x larger at the
        // top and the classifier would read the answer size, not the query.
        long[][] sweep = sweep("range-query/range", n -> {
            Treap<Long, Long> t = build(n);
            long w = rangeWidth(n);
            return keyed(n, i -> {
                long from = keyAt(i);
                RangeQuery.of(t, from, true, from + w, true).size();
            }, true);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Treap<Long, Long> t = build(CANON);
        long w = rangeWidth(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("range_scan", keyed(CANON, i -> {
            long from = keyAt(i);
            RangeQuery.of(t, from, true, from + w, true).size();
        }, false));
        manifest.setFeature("range-query", d.category(), p99, d.reason());
    }

    // ---------- persistent: path-copying insert, old version stays valid ----------
    private static void persistent(SubMsFeatureManifest manifest, long baseP50) {
        // `insert` returns a NEW treap sharing everything off the copied path, so
        // the cost is the path length - O(log n), which should read flat.
        long[][] sweep = sweep("persistent/insert", n -> {
            PersistentTreap<Long, Long> p = filledPersistent(n);
            return keyed(n, i -> p.insert(keyAt(i), (long) i), true);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        PersistentTreap<Long, Long> p = filledPersistent(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("insert", keyed(CANON, i -> p.insert(keyAt(i), (long) i), false));
        p99.put("get", keyed(CANON, i -> p.get(keyAt(i)), false));
        p99.put("remove", keyed(CANON, i -> p.remove(keyAt(i)), false));
        manifest.setFeature("persistent", d.category(), p99, d.reason());
    }

    private static PersistentTreap<Long, Long> filledPersistent(int n) {
        PersistentTreap<Long, Long> p = new PersistentTreap<>(SEED);
        for (int i = 0; i < n; i++) {
            p = p.insert(keyAt(i), (long) i);
        }
        return p;
    }

    // ---------- merge-split: split at a pivot, merge two ordered halves ----------
    private static void mergeSplit(SubMsFeatureManifest manifest, long baseP50) {
        // Timed as a split-then-merge ROUND TRIP, because `split` drains the
        // treap: rebuilding one per rep would put an O(n log n) build inside the
        // timed region and the figure would be the build. A round trip restores
        // the original, so the input is set up once and every rep does identical
        // work.
        //
        // The sweep classifies this structural, and the reason is in `split`
        // rather than in `splitNode`: the descent is O(log n), but split then
        // calls `count()` on BOTH halves to fill in their sizes, and that is a
        // full traversal. An O(log n) op with an O(n) bookkeeping tail.
        long[][] sweep = sweep("merge-split/split+merge",
                n -> bulk(() -> holder(n), PerfFeaturesMain::roundTrip, true));
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("split_merge", bulk(() -> holder(CANON), PerfFeaturesMain::roundTrip, false));
        manifest.setFeature("merge-split", d.category(), p99, d.reason());
    }

    @SuppressWarnings("unchecked")
    private static SplittableTreap<Long, Long>[] holder(int n) {
        SplittableTreap<Long, Long> t = new SplittableTreap<>(SEED);
        for (int i = 0; i < n; i++) {
            t.insert(keyAt(i), (long) i);
        }
        return new SplittableTreap[] {t};
    }

    private static void roundTrip(SplittableTreap<Long, Long>[] slot) {
        SplittableTreap.Split<Long, Long> s = slot[0].split(KEY_SPACE / 2);
        slot[0] = SplittableTreap.merge(s.left, s.right);
    }

    // ---------- concurrent-reads: a flattened immutable snapshot ----------
    private static void concurrentReads(SubMsFeatureManifest manifest, long baseP50) {
        // `fromTreap` flattens the tree into a sorted list, so it is O(n) and the
        // sweep says so. Lookups on the result are a binary search over that list,
        // which is the point: readers pay O(log n) with no tree pointers and no
        // coordination with the writer.
        long[][] sweep = sweep("concurrent-reads/snapshot", n -> {
            Treap<Long, Long> t = build(n);
            return bulk(() -> t, x -> TreapSnapshot.fromTreap(x), true);
        });
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Treap<Long, Long> t = build(CANON);
        TreapSnapshot<Long, Long> snap = TreapSnapshot.fromTreap(t);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("snapshot", bulk(() -> t, x -> TreapSnapshot.fromTreap(x), false));
        p99.put("lookup_on_snapshot", keyed(CANON, i -> snap.get(keyAt(i)), false));
        manifest.setFeature("concurrent-reads", d.category(), p99, d.reason());
    }

    // ---------- harness plumbing ----------

    /**
     * Sweeps and PRINTS the curve. A non-monotonic or ratio-compressed sweep
     * classifies flat, and the only way to catch one is to look at the rows.
     */
    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = at.at(SIZES[i]);
            sb.append('(').append(SIZES[i]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n);
    }

    /** p50/p99 (ns) of {@code op} over a fixed OPS of keys from a tree of size n. */
    private static long keyed(int n, IntConsumer op, boolean median) {
        SubMsPerfHarness h = new SubMsPerfHarness("treap-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        // Warm to C2 first. An unwarmed JIT costs most on the FIRST measured
        // size, which the sweep reads as a cost that FALLS with N - the opposite
        // of the structural signal, and just as wrong.
        for (int i = 0; i < OPS; i++) {
            op.accept((i * 7919) % n);
        }
        for (int i = 0; i < OPS; i++) {
            int idx = (i * 7919) % n;
            st.time(() -> op.accept(idx));
        }
        return stat(h, median);
    }

    private static <T> long bulk(Supplier<T> setup, java.util.function.Consumer<T> op,
            boolean median) {
        T input = setup.get();
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(input);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("treap-feature", "java");
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
