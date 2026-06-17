package com.submillisecond.recipes.hll;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.hll.features.SparseHyperLogLog;
import com.submillisecond.recipes.hll.features.UnionIntersect;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per (variant, operation) - base_add, base_estimate,
 * sparse_add, sparse_estimate, union, intersect - with the SAME stage
 * names as the Rust bench so the cookbook FeaturePicker columns line up
 * across languages. JSON contract goes to stdout.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.hll.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final int PRECISION = 14;

    // Every stage here is a register-saturating add or a read-only estimate /
    // set-op, so over-running the op during the untimed warmup pass cannot
    // corrupt the timing - re-adding a key just re-sets a register it already
    // owns. Warm to C2 before recording so the number is steady-state.
    private static final int WARMUP = Math.min(ENTRIES, 20_000);

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("hyperloglog-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("precision", Integer.toString(PRECISION));
        h.meta("subms.recipe.slug", "subms-hyperloglog");
        h.meta("subms.recipe.category", "probabilistic");

        String[] keys = new String[ENTRIES];
        for (int i = 0; i < ENTRIES; i++) keys[i] = "key-" + i;

        // ---------- base ----------
        h.meta("subms.workload.feature", "base");
        HyperLogLog base = new HyperLogLog(PRECISION);
        SubMsPerfHarness.Stage baseAdd = h.stage("base_add", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        baseAdd.warmThenTime(WARMUP, ENTRIES, (int i) -> base.add(keys[i % keys.length]));
        SubMsPerfHarness.Stage baseEst = h.stage("base_estimate", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        baseEst.warmThenTime(WARMUP, ENTRIES, (int i) -> consume(base.estimate()));

        // ---------- sparse ----------
        h.meta("subms.workload.feature", "sparse");
        SparseHyperLogLog sparse = new SparseHyperLogLog(PRECISION);
        SubMsPerfHarness.Stage spAdd = h.stage("sparse_add", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        spAdd.warmThenTime(WARMUP, ENTRIES, (int i) -> sparse.add(keys[i % keys.length]));
        SubMsPerfHarness.Stage spEst = h.stage("sparse_estimate", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        spEst.warmThenTime(WARMUP, ENTRIES, (int i) -> consume(sparse.estimate()));

        // ---------- union / intersect ----------
        // Build two half-overlapping sketches outside the timed loops so
        // the stage measures only the set-op cost, not the inserts.
        h.meta("subms.workload.feature", "union-intersect");
        HyperLogLog a = new HyperLogLog(PRECISION);
        HyperLogLog b = new HyperLogLog(PRECISION);
        for (int i = 0; i < ENTRIES; i++) a.add("a-" + i);
        for (int i = ENTRIES / 2; i < ENTRIES + ENTRIES / 2; i++) b.add("a-" + i);
        SubMsPerfHarness.Stage union = h.stage("union", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        union.warmThenTime(WARMUP, ENTRIES, (int i) -> consume(UnionIntersect.estimateUnion(a, b)));
        SubMsPerfHarness.Stage inter = h.stage("intersect", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        inter.warmThenTime(WARMUP, ENTRIES, (int i) -> consume(UnionIntersect.estimateIntersect(a, b)));

        h.writeJson(System.out);
    }

    private static double blackHole;

    /** Keep the optimiser from eliding an estimate / set-op whose result is unused. */
    private static void consume(double v) {
        blackHole += v;
    }
}
