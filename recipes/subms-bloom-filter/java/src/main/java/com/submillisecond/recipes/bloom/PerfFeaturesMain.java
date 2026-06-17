package com.submillisecond.recipes.bloom;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.bloom.features.CountingBloomFilter;
import com.submillisecond.recipes.bloom.features.PartitionedBloomFilter;
import com.submillisecond.recipes.bloom.features.ScalableBloomFilter;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Runs the same 50k-entry workload against the base {@link BloomFilter} plus
 * each feature variant (counting, scalable, partitioned), one stage per
 * (variant, operation) with the SAME stage names as the Rust bench so the
 * cookbook FeaturePicker columns line up across languages. JSON contract goes
 * to stdout.
 *
 * <p>The Rust side gates each variant behind a Cargo feature; Java ships them
 * all on the classpath, so every stage is always emitted here.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.bloom.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final long SEED = 0;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("bloom-filter-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Long.toString(SEED));
        h.meta("subms.recipe.slug", "subms-bloom-filter");
        h.meta("subms.recipe.category", "probabilistic");

        // Keys mirror Rust's bench_keyed_op ("k{u32}") and bench_templated_op
        // ("miss-{i}"). The keyed sequence is seeded once per stage so each
        // variant sees the same hit/add stream the Rust side does.

        // ---------- base ----------
        h.meta("subms.workload.feature", "base");
        BloomFilter bf = new BloomFilter(ENTRIES);
        keyedOp(h, "base_add", ENTRIES, SEED, bf::add, SubMsStageKind.HOT_PATH);
        keyedOp(h, "base_hit", ENTRIES, SEED, bf::mightContain, SubMsStageKind.HOT_PATH);
        templatedOp(h, "base_miss", ENTRIES, "miss-", bf::mightContain, SubMsStageKind.HOT_PATH);

        // ---------- counting ----------
        h.meta("subms.workload.feature", "counting");
        CountingBloomFilter cb = new CountingBloomFilter(ENTRIES);
        keyedOp(h, "counting_add", ENTRIES, SEED, cb::add, SubMsStageKind.HOT_PATH);
        keyedOp(h, "counting_hit", ENTRIES, SEED, cb::mightContain, SubMsStageKind.HOT_PATH);
        keyedOp(h, "counting_remove", ENTRIES, SEED, cb::remove, SubMsStageKind.HOT_PATH);

        // ---------- scalable ----------
        h.meta("subms.workload.feature", "scalable");
        ScalableBloomFilter sb = new ScalableBloomFilter(1_000);
        keyedOp(h, "scalable_add", ENTRIES, SEED, sb::add, SubMsStageKind.HOT_PATH);
        keyedOp(h, "scalable_hit", ENTRIES, SEED, sb::mightContain, SubMsStageKind.HOT_PATH);

        // ---------- partitioned ----------
        h.meta("subms.workload.feature", "partitioned");
        PartitionedBloomFilter pb = new PartitionedBloomFilter(ENTRIES);
        keyedOp(h, "partitioned_add", ENTRIES, SEED, pb::add, SubMsStageKind.HOT_PATH);
        keyedOp(h, "partitioned_hit", ENTRIES, SEED, pb::mightContain, SubMsStageKind.HOT_PATH);
        templatedOp(h, "partitioned_miss", ENTRIES, "miss-", pb::mightContain, SubMsStageKind.HOT_PATH);

        h.writeJson(System.out);
    }

    // All bloom variants saturate bits/counters rather than capacity-fail, so
    // re-running an op during the untimed warmup pass cannot corrupt the timing
    // - over-filling just sets bits that are already set. Pre-materialise the
    // key stream so warmup and the measured pass walk the same keys, and warm
    // to C2 before recording so the number is steady-state, not interpreter-cold.
    private static final int WARMUP = Math.min(ENTRIES, 20_000);

    private static void keyedOp(SubMsPerfHarness h, String name, int count, long seed, KeyOp op, SubMsStageKind kind) {
        Lcg rng = new Lcg(seed);
        String[] keys = new String[count];
        for (int i = 0; i < count; i++) {
            keys[i] = "k" + Integer.toUnsignedString(rng.nextU32());
        }
        SubMsPerfHarness.Stage stage = h.stage(name, count).withKind(kind);
        stage.warmThenTime(WARMUP, count, (int i) -> op.accept(keys[i % keys.length]));
    }

    private static void templatedOp(SubMsPerfHarness h, String name, int count, String template, KeyOp op, SubMsStageKind kind) {
        String[] keys = new String[count];
        for (int i = 0; i < count; i++) {
            keys[i] = template + i;
        }
        SubMsPerfHarness.Stage stage = h.stage(name, count).withKind(kind);
        stage.warmThenTime(WARMUP, count, (int i) -> op.accept(keys[i % keys.length]));
    }

    @FunctionalInterface
    private interface KeyOp {
        void accept(String key);
    }

    /** Deterministic LCG matching the central {@code subms::SubMsLcg}. */
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
