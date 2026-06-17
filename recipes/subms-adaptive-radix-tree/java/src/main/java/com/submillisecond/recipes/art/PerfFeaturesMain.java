package com.submillisecond.recipes.art;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.art.features.ArtSnapshot;
import com.submillisecond.recipes.art.features.Compaction;
import com.submillisecond.recipes.art.features.MeasuredArt;
import com.submillisecond.recipes.art.features.RangeScan;
import com.submillisecond.recipes.art.features.Serialize;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per feature operation - base_insert, serialize_write,
 * range_scan_range, etc. - with the SAME stage names as the Rust bench so
 * the cookbook FeaturePicker columns line up across languages. JSON
 * contract goes to stdout.
 *
 * <p>Two stage shapes appear here. Per-key ops (insert, lookup, snapshot
 * point-get, delete) record one sample per key over the full 50k universe.
 * Bulk ops (whole-tree serialize, full range scan, snapshot capture,
 * compaction pass) operate on the whole populated tree at once and are
 * repeated {@code BULK_REPS} times to build a distribution.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.art.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final int SEED = 0;
    private static final int BULK_REPS = 200;

    // Warmup budget for the read-only / idempotent stages that can warm on the
    // real structure. Insert and delete stages instead warm on a throwaway:
    // warming the insert path on the real tree would convert fresh inserts into
    // updates of keys already present, and warming delete would empty the tree
    // the measured loop reads. The point of warmup is only to drive the path to
    // C2 - the instance it runs on is irrelevant.
    private static final int WARMUP = Math.min(ENTRIES, 20_000);
    private static final int BULK_WARMUP = 50;

    private static byte[] keyAt(int i) {
        return ("key-" + i).getBytes(StandardCharsets.UTF_8);
    }

    private static void populate(Art<Long> tree) {
        for (int i = 0; i < ENTRIES; i++) {
            tree.insert(keyAt(i), (long) i);
        }
    }

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("adaptive-radix-tree-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Integer.toString(SEED));
        h.input("bulk_reps", Integer.toString(BULK_REPS));
        h.meta("subms.recipe.slug", "subms-adaptive-radix-tree");
        h.meta("subms.recipe.category", "ordered-index");

        byte[][] keys = new byte[ENTRIES][];
        for (int i = 0; i < ENTRIES; i++) keys[i] = keyAt(i);

        // ---------- base ----------
        {
            h.meta("subms.workload.feature", "base");
            // Warm the insert path on a throwaway so the measured loop still
            // builds the tree from empty - one fresh insert per key, not an
            // update of a key already present.
            Art<Long> scratch = new Art<>();
            for (int i = 0; i < WARMUP; i++) scratch.insert(keys[i % keys.length], (long) i);

            Art<Long> tree = new Art<>();
            SubMsPerfHarness.Stage insert = h.stage("base_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                final int idx = i;
                insert.time(() -> tree.insert(keys[idx], (long) idx));
            }

            SubMsPerfHarness.Stage lookup = h.stage("base_lookup", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            lookup.warmThenTime(WARMUP, ENTRIES, (int i) -> tree.get(keys[i % keys.length]));
        }

        // ---------- serialize ----------
        {
            h.meta("subms.workload.feature", "serialize");
            Art<Long> tree = new Art<>();
            populate(tree);

            // Whole-tree serialize/parse: legitimately O(N) and expected to
            // stay above 1 ms at 50k entries (a disclosed non-claim, not a hot
            // path). Warm anyway so the recorded number is the steady-state
            // pass rather than an interpreter-cold first run. Re-serialising the
            // same tree is idempotent, so warming on it is safe.
            byte[][] buf = new byte[1][];
            SubMsPerfHarness.Stage write = h.stage("serialize_write", BULK_REPS).withKind(SubMsStageKind.BATCH_OP);
            write.warmThenTime(BULK_WARMUP, BULK_REPS, () -> {
                try {
                    buf[0] = Serialize.writeToBytes(tree, Serialize.INT64);
                } catch (IOException e) {
                    throw new RuntimeException(e);
                }
            });

            SubMsPerfHarness.Stage read = h.stage("serialize_read", BULK_REPS).withKind(SubMsStageKind.BATCH_OP);
            read.warmThenTime(BULK_WARMUP, BULK_REPS, () -> {
                try {
                    Art<Long> restored = Serialize.parseBytes(buf[0], Serialize.INT64);
                    consume(restored.size());
                } catch (IOException e) {
                    throw new RuntimeException(e);
                }
            });
        }

        // ---------- range-scan ----------
        {
            h.meta("subms.workload.feature", "range-scan");
            Art<Long> tree = new Art<>();
            populate(tree);

            // Full unbounded scan visits every entry: O(N) and expected to stay
            // above 1 ms at 50k (disclosed non-claim). Warm so the number is
            // steady-state; the scan is read-only, so warming on the real tree
            // is safe.
            SubMsPerfHarness.Stage scan = h.stage("range_scan_range", BULK_REPS).withKind(SubMsStageKind.BATCH_OP);
            scan.warmThenTime(BULK_WARMUP, BULK_REPS, () -> {
                List<RangeScan.Entry<Long>> out =
                        RangeScan.range(tree, RangeScan.Bound.unbounded(), RangeScan.Bound.unbounded());
                consume(out.size());
            });
        }

        // ---------- concurrent-reads ----------
        {
            h.meta("subms.workload.feature", "concurrent-reads");
            Art<Long> tree = new Art<>();
            populate(tree);

            // O(N) snapshot copy of the whole tree: expected to stay above 1 ms
            // (disclosed non-claim). Idempotent re-copy, safe to warm on the
            // real tree.
            SubMsPerfHarness.Stage snapshot = h.stage("concurrent_reads_snapshot", BULK_REPS).withKind(SubMsStageKind.BATCH_OP);
            snapshot.warmThenTime(BULK_WARMUP, BULK_REPS, () -> {
                ArtSnapshot<Long> snap = ArtSnapshot.fromTree(tree);
                consume(snap.size());
            });

            ArtSnapshot<Long> snap = ArtSnapshot.fromTree(tree);
            SubMsPerfHarness.Stage getOnSnap = h.stage("concurrent_reads_get_on_snapshot", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            getOnSnap.warmThenTime(WARMUP, ENTRIES, (int i) -> snap.get(keys[i % keys.length]));
        }

        // ---------- metrics ----------
        {
            h.meta("subms.workload.feature", "metrics");
            // Warm the metered insert path on a throwaway so the measured loop
            // still builds from empty rather than updating existing keys.
            MeasuredArt<Long> scratch = new MeasuredArt<>();
            for (int i = 0; i < WARMUP; i++) scratch.insert(keys[i % keys.length], (long) i);

            MeasuredArt<Long> tree = new MeasuredArt<>();
            SubMsPerfHarness.Stage insert = h.stage("metrics_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                final int idx = i;
                insert.time(() -> tree.insert(keys[idx], (long) idx));
            }

            SubMsPerfHarness.Stage lookup = h.stage("metrics_lookup", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            lookup.warmThenTime(WARMUP, ENTRIES, (int i) -> tree.get(keys[i % keys.length]));
        }

        // ---------- compaction ----------
        {
            h.meta("subms.workload.feature", "compaction");
            // Per-key delete over a freshly populated tree. Warm the delete path
            // on a throwaway - warming on the real tree would empty it and the
            // measured loop would then time miss-path deletes instead of hits.
            Art<Long> scratch = new Art<>();
            populate(scratch);
            for (int i = 0; i < WARMUP; i++) Compaction.delete(scratch, keys[i % keys.length]);

            Art<Long> tree = new Art<>();
            populate(tree);
            SubMsPerfHarness.Stage delete = h.stage("compaction_delete", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                final int idx = i;
                delete.time(() -> Compaction.delete(tree, keys[idx]));
            }

            // Bulk compact pass. Each rep rebuilds a populated tree, deletes
            // a fraction, then times the compaction sweep over what's left.
            // O(N) over the surviving entries: expected to stay above 1 ms at
            // 50k (disclosed non-claim). The per-rep setup varies the input, so
            // warm with an untimed pre-pass of the same rebuild+delete+compact
            // shape rather than warmThenTime, driving the sweep to C2 first.
            for (int w = 0; w < BULK_WARMUP; w++) {
                Art<Long> warmDirty = new Art<>();
                populate(warmDirty);
                for (int i = 0; i < ENTRIES; i += 2) {
                    Compaction.delete(warmDirty, keyAt(i));
                }
                consume(Compaction.compact(warmDirty));
            }
            SubMsPerfHarness.Stage compact = h.stage("compaction_compact", BULK_REPS).withKind(SubMsStageKind.BATCH_OP);
            int samplesTaken = 0;
            while (samplesTaken < BULK_REPS) {
                Art<Long> dirty = new Art<>();
                populate(dirty);
                for (int i = 0; i < ENTRIES; i += 2) {
                    Compaction.delete(dirty, keyAt(i));
                }
                compact.time(() -> {
                    int changes = Compaction.compact(dirty);
                    consume(changes);
                });
                samplesTaken++;
            }
        }

        h.writeJson(System.out);
    }

    private static long blackHole;

    /** Keep the optimiser from eliding a bulk op whose result is unused. */
    private static void consume(long v) {
        blackHole += v;
    }
}
