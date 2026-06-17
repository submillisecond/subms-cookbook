package com.submillisecond.recipes.treap;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.treap.features.PersistentTreap;
import com.submillisecond.recipes.treap.features.RangeQuery;
import com.submillisecond.recipes.treap.features.SplittableTreap;
import com.submillisecond.recipes.treap.features.TreapSnapshot;

import java.io.IOException;
import java.util.Iterator;
import java.util.Map;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per feature variant - base_insert, range, persistent_insert,
 * split, etc. - with the SAME stage names as the Rust bench so the cookbook
 * FeaturePicker columns line up across languages. JSON contract goes to stdout.
 *
 * <p>Keys are {@code Long} so the ordered-structure features (range scans,
 * splits) exercise real BST-on-key ordering. The key universe is generated
 * from an LCG seeded off {@code SEED}, mirroring the Rust {@code SubMsLcg}
 * so the two languages walk the same key stream.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.treap.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final long SEED = 0;
    private static final int MS_ROUNDS = 2_000;
    private static final long WINDOW = 256;

    public static void main(String[] args) throws Exception {
        // The pointer-backed feature treaps recurse to tree depth on
        // insert/split/merge; a 50k-entry random treap can spike well past
        // the default main-thread stack. Run on a worker thread with a
        // generous stack so deep-but-bounded recursion stays within budget.
        final IOException[] err = new IOException[1];
        Thread worker = new Thread(null, () -> {
            try {
                run();
            } catch (IOException e) {
                err[0] = e;
            }
        }, "treap-perf-features", 256L * 1024 * 1024);
        worker.start();
        worker.join();
        if (err[0] != null) throw err[0];
    }

    private static long[] keys(long seed, int count) {
        Lcg rng = new Lcg(seed);
        long[] out = new long[count];
        for (int i = 0; i < count; i++) {
            out[i] = rng.nextU32() & 0xffffffffL;
        }
        return out;
    }

    private static void run() throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("treap-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Long.toString(SEED));
        h.meta("subms.recipe.slug", "subms-treap");
        h.meta("subms.recipe.category", "ordered-index");

        long[] keySet = keys(SEED, ENTRIES);

        // ---------- base ----------
        {
            h.meta("subms.workload.feature", "base");
            Treap<Long, Long> t = new Treap<>(SEED);
            SubMsPerfHarness.Stage insert = h.stage("base_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (long k : keySet) {
                final long key = k;
                insert.time(() -> t.insert(key, key));
            }
            SubMsPerfHarness.Stage get = h.stage("base_get", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (long k : keySet) {
                final long key = k;
                get.time(() -> t.get(key));
            }
        }

        // ---------- range-query ----------
        {
            h.meta("subms.workload.feature", "range-query");
            // Dense keys 0..ENTRIES so a fixed-width window always lands on a
            // populated stretch - sparse u32 keys would make almost every
            // window empty and `next` never advance.
            Treap<Long, Long> t = new Treap<>(SEED);
            for (long k = 0; k < ENTRIES; k++) {
                t.insert(k, k);
            }

            // `range` times the boundary descent + first element pull: the
            // O(log T) locate cost. `next` times each subsequent in-order
            // step across a fixed-width populated window so it isolates the
            // amortised O(1) advance from the one-off seek.
            long maxStart = ENTRIES - WINDOW - 1;

            // warmThenTime warms to C2 before recording. Cold C1 code cannot
            // run escape analysis, so the per-call RangeQuery + iterator
            // allocate on the heap and the p99 catches GC pauses instead of
            // the O(log T) descent it means to measure. After C2 the iterator
            // is stack-allocated and the number reflects steady-state.
            SubMsPerfHarness.Stage range = h.stage("range", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            Lcg rng = new Lcg(SEED ^ 0x9e37L);
            range.warmThenTime(20_000, ENTRIES, (int idx) -> {
                long from = (rng.nextU32() & 0xffffffffL) % maxStart;
                long to = from + WINDOW;
                RangeQuery<Long, Long> it = RangeQuery.of(t, from, true, to, true);
                Iterator<Map.Entry<Long, Long>> iter = it.iterator();
                if (iter.hasNext()) iter.next();
            });

            // Record exactly ENTRIES timed `next()` advances, opening a fresh
            // window whenever the current iterator runs dry. Each window holds
            // WINDOW+1 dense keys, so progress is guaranteed.
            SubMsPerfHarness.Stage next = h.stage("next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            Lcg rng2 = new Lcg(SEED ^ 0x9e37L);
            int recorded = 0;
            outer:
            while (true) {
                long from = (rng2.nextU32() & 0xffffffffL) % maxStart;
                long to = from + WINDOW;
                RangeQuery<Long, Long> q = RangeQuery.of(t, from, true, to, true);
                Iterator<Map.Entry<Long, Long>> iter = q.iterator();
                if (iter.hasNext()) iter.next();
                while (true) {
                    long t0 = SubMsTimer.nanosNow();
                    boolean advanced = iter.hasNext();
                    if (advanced) iter.next();
                    next.record(SubMsTimer.nanosNow() - t0);
                    if (!advanced) break;
                    recorded++;
                    if (recorded >= ENTRIES) break outer;
                }
            }
        }

        // ---------- persistent ----------
        {
            h.meta("subms.workload.feature", "persistent");
            // `insert` returns a NEW version each call; chaining them grows
            // a version chain so the path-copy cost is measured against a
            // realistically-sized tree.
            PersistentTreap<Long, Long>[] holder = newHolder(new PersistentTreap<>(SEED));
            SubMsPerfHarness.Stage insert = h.stage("persistent_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (long k : keySet) {
                final long key = k;
                insert.time(() -> holder[0] = holder[0].insert(key, key));
            }

            PersistentTreap<Long, Long> finalVersion = holder[0];
            SubMsPerfHarness.Stage get = h.stage("persistent_get", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (long k : keySet) {
                final long key = k;
                get.time(() -> finalVersion.get(key));
            }
        }

        // ---------- merge-split ----------
        {
            h.meta("subms.workload.feature", "merge-split");
            // split + merge both consume the treap and hand it back, so the
            // round-trip is: split at a pivot, then merge the halves back.
            // `split` recomputes both halves' lengths with a full traversal,
            // so each round-trip is O(N) at this 50k size - we run a bounded
            // number of rounds rather than ENTRIES, which would be quadratic.
            SplittableTreap<Long, Long> tree = new SplittableTreap<>(SEED);
            for (long k : keySet) {
                tree.insert(k, k);
            }

            long[] pivots = keys(SEED ^ 0x5151L, MS_ROUNDS);

            long[] splitSamples = new long[MS_ROUNDS];
            long[] mergeSamples = new long[MS_ROUNDS];

            // Warm split + merge to C2 before the measured rounds. split and
            // merge are interleaved into two stages, so they take a manual
            // warmup pre-pass rather than warmThenTime; without it the first
            // few hundred interpreter-cold samples dominate the O(N) p99.
            for (int i = 0; i < 500; i++) {
                SplittableTreap.Split<Long, Long> sp = tree.split(pivots[i % MS_ROUNDS]);
                tree = SplittableTreap.merge(sp.left, sp.right);
            }

            for (int i = 0; i < MS_ROUNDS; i++) {
                long pivot = pivots[i];

                long t0 = SubMsTimer.nanosNow();
                SplittableTreap.Split<Long, Long> sp = tree.split(pivot);
                splitSamples[i] = SubMsTimer.nanosNow() - t0;

                long t1 = SubMsTimer.nanosNow();
                tree = SplittableTreap.merge(sp.left, sp.right);
                mergeSamples[i] = SubMsTimer.nanosNow() - t1;
            }

            SubMsPerfHarness.Stage split = h.stage("split", splitSamples.length).withKind(SubMsStageKind.BATCH_OP);
            for (long ns : splitSamples) split.record(ns);
            SubMsPerfHarness.Stage merge = h.stage("merge", mergeSamples.length).withKind(SubMsStageKind.BATCH_OP);
            for (long ns : mergeSamples) merge.record(ns);
        }

        // ---------- concurrent-reads ----------
        {
            h.meta("subms.workload.feature", "concurrent-reads");
            Treap<Long, Long> t = new Treap<>(SEED);
            for (long k : keySet) {
                t.insert(k, k);
            }

            // `snapshot` is a one-shot O(N) capture. Warm to C2 first - a
            // 32-sample cold stage reads pure interpreter startup, not the
            // steady-state copy cost. Still not a per-op hot path, so the
            // measured count stays modest.
            SubMsPerfHarness.Stage snapshot = h.stage("snapshot", 200).withKind(SubMsStageKind.BATCH_OP);
            TreapSnapshot<Long, Long>[] snapHolder = newSnapHolder(TreapSnapshot.fromTreap(t));
            snapshot.warmThenTime(200, 200, () -> snapHolder[0] = TreapSnapshot.fromTreap(t));

            TreapSnapshot<Long, Long> snap = snapHolder[0];
            SubMsPerfHarness.Stage get = h.stage("get_on_snapshot", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (long k : keySet) {
                final long key = k;
                get.time(() -> snap.get(key));
            }
        }

        h.writeJson(System.out);
    }

    @SuppressWarnings("unchecked")
    private static PersistentTreap<Long, Long>[] newHolder(PersistentTreap<Long, Long> v) {
        PersistentTreap<Long, Long>[] h = (PersistentTreap<Long, Long>[]) new PersistentTreap[1];
        h[0] = v;
        return h;
    }

    @SuppressWarnings("unchecked")
    private static TreapSnapshot<Long, Long>[] newSnapHolder(TreapSnapshot<Long, Long> v) {
        TreapSnapshot<Long, Long>[] h = (TreapSnapshot<Long, Long>[]) new TreapSnapshot[1];
        h[0] = v;
        return h;
    }

    /** Deterministic LCG, the Java mirror of {@code subms::SubMsLcg}. */
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed | 1L;
        }

        int nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> 32);
        }
    }
}
