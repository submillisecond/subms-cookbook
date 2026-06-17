package com.submillisecond.recipes.cuckoo;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.cuckoo.features.CompressedCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.CuckooSnapshot;
import com.submillisecond.recipes.cuckoo.features.DynamicCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per (variant, operation) - base_insert, base_lookup,
 * base_delete, variable_fingerprint_insert/lookup/delete,
 * dynamic_insert/lookup/delete, snapshot, lookup_on_snapshot,
 * compressed_buckets_insert/lookup/delete - with the SAME stage names as
 * the Rust bench so the cookbook FeaturePicker columns line up across
 * languages. JSON contract goes to stdout.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.cuckoo.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;

    // Warmup iteration budget for the read-only / idempotent stages that can
    // safely warm on the real structure. For inserts and deletes we warm on a
    // throwaway instead (see warmInserts / warmDeletes) - re-running an insert
    // past the filter's load factor would start timing the kick-limit failure
    // path, and a delete sweep would empty the real filter, so neither may run
    // extra rounds on the structure the measured loop reads.
    private static final int WARMUP = Math.min(ENTRIES, 20_000);

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("cuckoo-filter-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", "0");
        h.meta("subms.recipe.slug", "subms-cuckoo-filter");
        h.meta("subms.recipe.category", "probabilistic");

        String[] keys = new String[ENTRIES];
        for (int i = 0; i < ENTRIES; i++) keys[i] = "key-" + i;

        // ---------- base ----------
        h.meta("subms.workload.feature", "base");
        warmInserts(new CuckooFilter(ENTRIES), keys);
        CuckooFilter cf = new CuckooFilter(ENTRIES);
        SubMsPerfHarness.Stage baseIns = h.stage("base_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) baseIns.time(() -> cf.insert(k));
        SubMsPerfHarness.Stage baseLook = h.stage("base_lookup", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        baseLook.warmThenTime(WARMUP, ENTRIES, (int i) -> cf.contains(keys[i % keys.length]));
        warmDeletes(new CuckooFilter(ENTRIES), keys);
        SubMsPerfHarness.Stage baseDel = h.stage("base_delete", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) baseDel.time(() -> cf.delete(k));

        // ---------- variable-fingerprint ----------
        // Sixteen-bit fingerprint: the widest option, the headline
        // memory-for-FPR tradeoff this feature exists to expose.
        h.meta("subms.workload.feature", "variable-fingerprint");
        warmInserts(new VariableFpCuckooFilter(ENTRIES, FingerprintWidth.SIXTEEN), keys);
        VariableFpCuckooFilter vf = new VariableFpCuckooFilter(ENTRIES, FingerprintWidth.SIXTEEN);
        SubMsPerfHarness.Stage vfIns = h.stage("variable_fingerprint_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) vfIns.time(() -> vf.insert(k));
        SubMsPerfHarness.Stage vfLook = h.stage("variable_fingerprint_lookup", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        vfLook.warmThenTime(WARMUP, ENTRIES, (int i) -> vf.contains(keys[i % keys.length]));
        warmDeletes(new VariableFpCuckooFilter(ENTRIES, FingerprintWidth.SIXTEEN), keys);
        SubMsPerfHarness.Stage vfDel = h.stage("variable_fingerprint_delete", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) vfDel.time(() -> vf.delete(k));

        // ---------- dynamic ----------
        // Dynamic grows by appending sub-filters rather than failing, but
        // over-running inserts would balloon it with extra sub-filters and skew
        // the timing - warm on a throwaway like the fixed-capacity variants.
        h.meta("subms.workload.feature", "dynamic");
        warmDynInserts(new DynamicCuckooFilter(ENTRIES), keys);
        DynamicCuckooFilter dy = new DynamicCuckooFilter(ENTRIES);
        SubMsPerfHarness.Stage dyIns = h.stage("dynamic_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) dyIns.time(() -> dy.insert(k));
        SubMsPerfHarness.Stage dyLook = h.stage("dynamic_lookup", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        dyLook.warmThenTime(WARMUP, ENTRIES, (int i) -> dy.contains(keys[i % keys.length]));
        warmDynDeletes(new DynamicCuckooFilter(ENTRIES), keys);
        SubMsPerfHarness.Stage dyDel = h.stage("dynamic_delete", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) dyDel.time(() -> dy.delete(k));

        // ---------- concurrent-reads ----------
        // Populate the source filter untimed, then time the one-shot snapshot
        // capture - a single O(N) bucket copy - followed by per-key lookups
        // against the frozen snapshot. capture is idempotent (it re-copies the
        // same buckets), so warmThenTime on the real source is safe; the stage
        // stays O(N) and is a disclosed non-claim, but warmup makes the number
        // steady-state rather than a cold count=1 interpreter pass.
        h.meta("subms.workload.feature", "concurrent-reads");
        CuckooFilter snapSrc = new CuckooFilter(ENTRIES);
        for (String k : keys) snapSrc.insert(k);
        CuckooSnapshot[] holder = new CuckooSnapshot[1];
        SubMsPerfHarness.Stage snapStage = h.stage("snapshot", 200).withKind(SubMsStageKind.BATCH_OP);
        snapStage.warmThenTime(200, 200, () -> holder[0] = CuckooSnapshot.capture(snapSrc));
        CuckooSnapshot snap = holder[0];
        SubMsPerfHarness.Stage snapLook = h.stage("lookup_on_snapshot", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        snapLook.warmThenTime(WARMUP, ENTRIES, (int i) -> snap.contains(keys[i % keys.length]));

        // ---------- compressed-buckets ----------
        h.meta("subms.workload.feature", "compressed-buckets");
        warmInserts(new CompressedCuckooFilter(ENTRIES), keys);
        CompressedCuckooFilter cb = new CompressedCuckooFilter(ENTRIES);
        SubMsPerfHarness.Stage cbIns = h.stage("compressed_buckets_insert", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) cbIns.time(() -> cb.insert(k));
        SubMsPerfHarness.Stage cbLook = h.stage("compressed_buckets_lookup", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        cbLook.warmThenTime(WARMUP, ENTRIES, (int i) -> cb.contains(keys[i % keys.length]));
        warmDeletes(new CompressedCuckooFilter(ENTRIES), keys);
        SubMsPerfHarness.Stage cbDel = h.stage("compressed_buckets_delete", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) cbDel.time(() -> cb.delete(k));

        h.writeJson(System.out);
    }

    // Warm the insert path to C2 on a throwaway filter. Stops short of the load
    // factor so warmup itself never hits the kick-limit failure path.
    private static void warmInserts(CuckooFilter scratch, String[] keys) {
        for (int i = 0; i < WARMUP; i++) scratch.insert(keys[i % keys.length]);
    }

    private static void warmInserts(VariableFpCuckooFilter scratch, String[] keys) {
        for (int i = 0; i < WARMUP; i++) scratch.insert(keys[i % keys.length]);
    }

    private static void warmInserts(CompressedCuckooFilter scratch, String[] keys) {
        for (int i = 0; i < WARMUP; i++) scratch.insert(keys[i % keys.length]);
    }

    private static void warmDynInserts(DynamicCuckooFilter scratch, String[] keys) {
        for (int i = 0; i < WARMUP; i++) scratch.insert(keys[i % keys.length]);
    }

    // Warm the delete path to C2 on a throwaway: populate, then delete the same
    // keys so each timed-equivalent call exercises a real hit-then-remove, not
    // an empty-slot miss.
    private static void warmDeletes(CuckooFilter scratch, String[] keys) {
        int n = Math.min(WARMUP, keys.length);
        for (int i = 0; i < n; i++) scratch.insert(keys[i]);
        for (int i = 0; i < n; i++) scratch.delete(keys[i]);
    }

    private static void warmDeletes(VariableFpCuckooFilter scratch, String[] keys) {
        int n = Math.min(WARMUP, keys.length);
        for (int i = 0; i < n; i++) scratch.insert(keys[i]);
        for (int i = 0; i < n; i++) scratch.delete(keys[i]);
    }

    private static void warmDeletes(CompressedCuckooFilter scratch, String[] keys) {
        int n = Math.min(WARMUP, keys.length);
        for (int i = 0; i < n; i++) scratch.insert(keys[i]);
        for (int i = 0; i < n; i++) scratch.delete(keys[i]);
    }

    private static void warmDynDeletes(DynamicCuckooFilter scratch, String[] keys) {
        int n = Math.min(WARMUP, keys.length);
        for (int i = 0; i < n; i++) scratch.insert(keys[i]);
        for (int i = 0; i < n; i++) scratch.delete(keys[i]);
    }
}
