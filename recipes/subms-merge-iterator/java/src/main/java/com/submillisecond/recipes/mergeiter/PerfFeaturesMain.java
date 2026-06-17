package com.submillisecond.recipes.mergeiter;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.mergeiter.features.DedupEntry;
import com.submillisecond.recipes.mergeiter.features.DedupMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PriorityEntry;
import com.submillisecond.recipes.mergeiter.features.PriorityMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PrioritySource;
import com.submillisecond.recipes.mergeiter.features.SeekableMergeIterator;
import com.submillisecond.recipes.mergeiter.features.TombstoneEntry;
import com.submillisecond.recipes.mergeiter.features.TombstoneMergeIterator;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per variant - base_next, seek, next_after_seek,
 * tombstones_next, dedup_next, priority_next - with the SAME stage names
 * as the Rust bench so the cookbook FeaturePicker columns line up across
 * languages. JSON contract goes to stdout.
 *
 * <p>Every variant merges 16 sorted streams of ENTRIES/16 entries each.
 * The source lists are built outside the timed loop; only the merge step
 * (next / seek) is timed.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.mergeiter.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final int STREAMS = 16;
    private static final int SEED = 0;

    private static final int PER_STREAM = ENTRIES / STREAMS;
    private static final int TOTAL = PER_STREAM * STREAMS;

    /** Stream {@code s} carries values {@code s, s+STREAMS, s+2*STREAMS, ...}
     *  so the 16 streams interleave into the dense range {@code 0..TOTAL}
     *  with no gaps. */
    private static List<Iterator<Long>> plainStreams() {
        List<Iterator<Long>> streams = new ArrayList<>(STREAMS);
        for (int s = 0; s < STREAMS; s++) {
            List<Long> values = new ArrayList<>(PER_STREAM);
            for (int i = 0; i < PER_STREAM; i++) {
                values.add((long) (s + i * STREAMS));
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("merge-iterator-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("streams", Integer.toString(STREAMS));
        h.input("seed", Integer.toString(SEED));
        h.meta("per_stream", Integer.toString(PER_STREAM));
        h.meta("subms.recipe.slug", "subms-merge-iterator");
        h.meta("subms.recipe.category", "storage");

        // ---------- base ----------
        {
            h.meta("subms.workload.feature", "base");
            // Merge iterators are single-use, so warm next() on a throwaway
            // iterator over the same streams, then measure on a fresh one.
            MergeIterator<Long> warm = new MergeIterator<>(plainStreams());
            for (int i = 0; i < TOTAL; i++) {
                if (warm.hasNext()) warm.next();
            }

            MergeIterator<Long> iter = new MergeIterator<>(plainStreams());
            SubMsPerfHarness.Stage s = h.stage("base_next", TOTAL).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < TOTAL; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                if (iter.hasNext()) iter.next();
                s.record(t0.elapsedNs());
            }
        }

        // ---------- seek-to ----------
        {
            h.meta("subms.workload.feature", "seek-to");
            // Spread SEEKS targets evenly across the key range, then time a
            // seek + the next() that lands on the first key >= target. Each
            // seek advances every stream head forward, so we get one timed
            // pair per target.
            final int SEEKS = 4_000;
            long step = Math.max(1, TOTAL / SEEKS);
            long[] targets = new long[SEEKS];
            for (int i = 0; i < SEEKS; i++) targets[i] = i * step;

            // Seekable iterators are single-use; warm seek + next on a
            // throwaway over the same targets, then measure on a fresh one.
            SeekableMergeIterator<Long> warm = new SeekableMergeIterator<>(plainStreams());
            for (int i = 0; i < SEEKS; i++) {
                warm.seek(targets[i]);
                if (warm.hasNext()) warm.next();
            }

            SeekableMergeIterator<Long> iter = new SeekableMergeIterator<>(plainStreams());
            long[] seekTimes = new long[SEEKS];
            long[] afterTimes = new long[SEEKS];
            for (int i = 0; i < SEEKS; i++) {
                long target = targets[i];
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                iter.seek(target);
                seekTimes[i] = t0.elapsedNs();

                SubMsTimer.SubMsTick t1 = SubMsTimer.tick();
                if (iter.hasNext()) iter.next();
                afterTimes[i] = t1.elapsedNs();
            }
            SubMsPerfHarness.Stage sSeek = h.stage("seek", SEEKS).withKind(SubMsStageKind.HOT_PATH);
            for (long ns : seekTimes) sSeek.record(ns);
            SubMsPerfHarness.Stage sAfter = h.stage("next_after_seek", SEEKS).withKind(SubMsStageKind.HOT_PATH);
            for (long ns : afterTimes) sAfter.record(ns);
        }

        // ---------- tombstones ----------
        {
            h.meta("subms.workload.feature", "tombstones");
            // Tombstone iterators are single-use; warm next() on a throwaway
            // built from the same streams, then measure on a fresh one.
            TombstoneMergeIterator<Long, Long> warm = new TombstoneMergeIterator<>(tombstoneStreams());
            for (int i = 0; i < TOTAL; i++) {
                if (!warm.hasNext()) break;
                warm.next();
            }

            TombstoneMergeIterator<Long, Long> iter = new TombstoneMergeIterator<>(tombstoneStreams());
            SubMsPerfHarness.Stage s = h.stage("tombstones_next", TOTAL).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < TOTAL; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                boolean yielded = iter.hasNext();
                if (yielded) iter.next();
                s.record(t0.elapsedNs());
                if (!yielded) break;
            }
        }

        // ---------- dedup ----------
        {
            h.meta("subms.workload.feature", "dedup");
            // Dedup iterators are single-use; warm next() on a throwaway
            // built from the same streams, then measure on a fresh one.
            DedupMergeIterator<Long, Long> warm = new DedupMergeIterator<>(dedupStreams());
            for (int i = 0; i < TOTAL; i++) {
                if (!warm.hasNext()) break;
                warm.next();
            }

            DedupMergeIterator<Long, Long> iter = new DedupMergeIterator<>(dedupStreams());
            SubMsPerfHarness.Stage s = h.stage("dedup_next", TOTAL).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < TOTAL; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                boolean yielded = iter.hasNext();
                if (yielded) iter.next();
                s.record(t0.elapsedNs());
                if (!yielded) break;
            }
        }

        // ---------- priority ----------
        {
            h.meta("subms.workload.feature", "priority");
            // Priority iterators are single-use; warm next() on a throwaway
            // built from the same sources, then measure on a fresh one.
            PriorityMergeIterator<Long, Long> warm = new PriorityMergeIterator<>(prioritySources());
            for (int i = 0; i < TOTAL; i++) {
                if (!warm.hasNext()) break;
                warm.next();
            }

            PriorityMergeIterator<Long, Long> iter = new PriorityMergeIterator<>(prioritySources());
            SubMsPerfHarness.Stage s = h.stage("priority_next", TOTAL).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < TOTAL; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                boolean yielded = iter.hasNext();
                if (yielded) iter.next();
                s.record(t0.elapsedNs());
                if (!yielded) break;
            }
        }

        h.writeJson(System.out);
    }

    /** Every 8th key in each stream is a tombstone; the rest are live, so
     *  next() skips shadowed + tombstoned keys per pop. */
    private static List<Iterator<TombstoneEntry<Long, Long>>> tombstoneStreams() {
        List<Iterator<TombstoneEntry<Long, Long>>> streams = new ArrayList<>(STREAMS);
        for (int st = 0; st < STREAMS; st++) {
            List<TombstoneEntry<Long, Long>> values = new ArrayList<>(PER_STREAM);
            for (int i = 0; i < PER_STREAM; i++) {
                long key = st + i * STREAMS;
                if (key % 8 == 0) {
                    values.add(TombstoneEntry.tombstone(key));
                } else {
                    values.add(TombstoneEntry.live(key, key));
                }
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    /** Halve the key space so adjacent streams collide, exercising the
     *  same-key collapse path on next(). */
    private static List<Iterator<DedupEntry<Long, Long>>> dedupStreams() {
        List<Iterator<DedupEntry<Long, Long>>> streams = new ArrayList<>(STREAMS);
        for (int st = 0; st < STREAMS; st++) {
            List<DedupEntry<Long, Long>> values = new ArrayList<>(PER_STREAM);
            for (int i = 0; i < PER_STREAM; i++) {
                long key = ((long) (st + i * STREAMS)) / 2;
                values.add(new DedupEntry<>(key, key));
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    /** Same collide-on-halved-keys shape as dedup, with an explicit
     *  descending priority per source so next() runs the priority tie-break
     *  on every collapsed key. */
    private static List<PrioritySource<Long, Long>> prioritySources() {
        List<PrioritySource<Long, Long>> sources = new ArrayList<>(STREAMS);
        for (int st = 0; st < STREAMS; st++) {
            List<PriorityEntry<Long, Long>> values = new ArrayList<>(PER_STREAM);
            for (int i = 0; i < PER_STREAM; i++) {
                long key = ((long) (st + i * STREAMS)) / 2;
                values.add(new PriorityEntry<>(key, key));
            }
            sources.add(new PrioritySource<>(STREAMS - st, values.iterator()));
        }
        return sources;
    }
}
