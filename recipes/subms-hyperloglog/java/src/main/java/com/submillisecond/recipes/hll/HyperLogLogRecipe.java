package com.submillisecond.recipes.hll;

import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class HyperLogLogRecipe implements SubMsRecipe {

    /**
     * Samples for the {@code estimate} stage. The harness takes p99 as
     * {@code sorted[floor(0.99 * n)]}, so at n &lt;= 100 that index is
     * {@code n - 1} and the reported p99 is whichever single call caught a C2
     * compilation or a scheduler hiccup. At 100 samples this stage published a
     * 7.83 ms p99 against a 21 us steady state. 2000 puts 19 samples above the
     * p99 index and 1 above p999. Do not lower it.
     */
    private static final int ESTIMATE_SAMPLES = 2_000;

    /**
     * Untimed {@code estimate} warm-up, time-boxed rather than a fixed rep
     * count so it covers C2 compilation of a whole-array fold on a slow box
     * without spending the budget on a fast one. The Rust port uses a fixed 64
     * reps; there is no JIT to outrun there.
     */
    private static final long ESTIMATE_WARM_NANOS = 300_000_000L;
    private static final int ESTIMATE_WARM_MAX_REPS = 5_000;

    @Override
    public String name() {
        return "hyperloglog";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        HyperLogLog hll = new HyperLogLog(14);

        Random r = new Random(seed);
        for (int i = 0; i < warmup; i++) hll.add("warm" + r.nextInt());

        SubMsPerfHarness.Stage add = h.stage("add", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r2 = new Random(seed + 1);
        for (int i = 0; i < entries; i++) {
            String key = "k" + r2.nextInt();
            long t0 = SubMsTimer.nanosNow();
            hll.add(key);
            add.record(SubMsTimer.nanosNow() - t0);
        }

        long deadline = System.nanoTime() + ESTIMATE_WARM_NANOS;
        for (int i = 0; i < ESTIMATE_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            hll.estimate();
        }
        SubMsPerfHarness.Stage est =
                h.stage("estimate", ESTIMATE_SAMPLES).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ESTIMATE_SAMPLES; i++) {
            long t0 = SubMsTimer.nanosNow();
            hll.estimate();
            est.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("precision", "14");
        h.meta("registers", Integer.toString(hll.registerCount()));
        h.meta("estimate", Long.toString((long) hll.estimate()));
    }
}
