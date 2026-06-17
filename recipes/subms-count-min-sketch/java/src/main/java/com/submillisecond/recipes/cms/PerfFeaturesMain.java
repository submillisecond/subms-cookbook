package com.submillisecond.recipes.cms;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.cms.features.HeavyHitters;
import com.submillisecond.recipes.cms.features.Merge;
import com.submillisecond.recipes.cms.features.WindowedCountMinSketch;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per (variant, operation) - base_add, base_estimate,
 * heavy_hitters_add, heavy_hitters_top_k, windowed_add, windowed_estimate,
 * windowed_tick, merge_build_dst, merge_build_src, merge - with the SAME
 * stage names as the Rust bench so the cookbook FeaturePicker columns line
 * up across languages. JSON contract goes to stdout.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.cms.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final int DEPTH = 5;
    private static final int WIDTH = 16384;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("count-min-sketch-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", "0");
        h.meta("subms.recipe.slug", "subms-count-min-sketch");
        h.meta("subms.recipe.category", "probabilistic");

        String[] keys = new String[ENTRIES];
        for (int i = 0; i < ENTRIES; i++) keys[i] = "key-" + i;

        // ---------- base ----------
        h.meta("subms.workload.feature", "base");
        CountMinSketch cms = new CountMinSketch(DEPTH, WIDTH);
        SubMsPerfHarness.Stage baseAdd = h.stage("base_add", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) baseAdd.time(() -> cms.add(k));
        SubMsPerfHarness.Stage baseEst = h.stage("base_estimate", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) baseEst.time(() -> cms.estimate(k));

        // ---------- heavy-hitters ----------
        h.meta("subms.workload.feature", "heavy-hitters");
        HeavyHitters hh = new HeavyHitters(16, DEPTH, WIDTH);
        SubMsPerfHarness.Stage hhAdd = h.stage("heavy_hitters_add", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) hhAdd.time(() -> hh.add(k));
        SubMsPerfHarness.Stage hhTop = h.stage("heavy_hitters_top_k", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) hhTop.time(hh::top);

        // ---------- windowed ----------
        h.meta("subms.workload.feature", "windowed");
        WindowedCountMinSketch w = new WindowedCountMinSketch(4, DEPTH, WIDTH);
        SubMsPerfHarness.Stage wAdd = h.stage("windowed_add", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) wAdd.time(() -> w.add(k));
        SubMsPerfHarness.Stage wEst = h.stage("windowed_estimate", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) wEst.time(() -> w.estimate(k));
        SubMsPerfHarness.Stage wTick = h.stage("windowed_tick", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) wTick.time(w::tick);

        // ---------- merge ----------
        // Build both sketches outside the timed region. The merge stage
        // times a single element-wise max pass over d*w cells.
        h.meta("subms.workload.feature", "merge");
        CountMinSketch dst = new CountMinSketch(DEPTH, WIDTH);
        CountMinSketch src = new CountMinSketch(DEPTH, WIDTH);
        SubMsPerfHarness.Stage mergeDst = h.stage("merge_build_dst", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (String k : keys) mergeDst.time(() -> dst.add(k));
        SubMsPerfHarness.Stage mergeSrc = h.stage("merge_build_src", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ENTRIES; i++) {
            final String k = "src-" + i;
            mergeSrc.time(() -> src.add(k));
        }
        // mergeInto is an element-wise max over d*w cells and is idempotent
        // once dst >= src, so it is safe to run repeatedly. warmThenTime warms
        // the JIT to C2 before recording, so the number is the steady-state
        // pass production sees - not the cold count=1 interpreter startup that
        // read as multi-millisecond.
        final int MERGE_WARMUP = 500;
        final int MERGE_ROUNDS = 2_000;
        SubMsPerfHarness.Stage merge = h.stage("merge", MERGE_ROUNDS).withKind(SubMsStageKind.BATCH_OP);
        merge.warmThenTime(MERGE_WARMUP, MERGE_ROUNDS, () -> Merge.mergeInto(dst, src));

        h.writeJson(System.out);
    }
}
