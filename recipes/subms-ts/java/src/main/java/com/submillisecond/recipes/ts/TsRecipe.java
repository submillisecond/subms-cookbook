package com.submillisecond.recipes.ts;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives a representative subms-ts workload over a scalar-{@code double}
 * series ({@link TsSeriesD}): build (push), point lookup (nearest), and two
 * ranged aggregates over a fixed window. Stages mirror the Rust recipe:
 * {@code push}, {@code nearest}, {@code range_min}, {@code range_sum}.
 */
public final class TsRecipe implements SubMsRecipe {

    private static final long WINDOW = 256L;

    // Mirrors subms::SubMsLcg so the workload's pseudo-random draws match the
    // Rust recipe's drive sequence.
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed;
        }

        int nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> 32);
        }
    }

    @Override
    public String name() {
        return "subms-ts";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();

        TsSeriesD s = TsSeriesD.withCapacity(entries);
        SubMsPerfHarness.Stage push = h.stage("push", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng = new Lcg(seed);
        for (int i = 0; i < entries; i++) {
            double v = (rng.nextU32() & 0xffffffffL) / (double) 0xffffffffL;
            long t0 = SubMsTimer.nanosNow();
            s.push(i, v);
            push.record(SubMsTimer.nanosNow() - t0);
        }

        long span = Math.max(1, entries);
        long maxStart = Math.max(0, span - WINDOW - 1);

        SubMsPerfHarness.Stage near = h.stage("nearest", entries).withKind(SubMsStageKind.HOT_PATH);
        rng = new Lcg(seed ^ 0x1234L);
        for (int i = 0; i < entries; i++) {
            long target = Math.floorMod((long) rng.nextU32(), span);
            long t0 = SubMsTimer.nanosNow();
            s.nearest(target);
            near.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage rmin = h.stage("range_min", entries).withKind(SubMsStageKind.HOT_PATH);
        rng = new Lcg(seed ^ 0x5678L);
        for (int i = 0; i < entries; i++) {
            long from = Math.floorMod((long) rng.nextU32(), maxStart + 1);
            long t0 = SubMsTimer.nanosNow();
            s.rangeMin(from, from + WINDOW);
            rmin.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage rsum = h.stage("range_sum", entries).withKind(SubMsStageKind.HOT_PATH);
        rng = new Lcg(seed ^ 0x9abcL);
        for (int i = 0; i < entries; i++) {
            long from = Math.floorMod((long) rng.nextU32(), maxStart + 1);
            long t0 = SubMsTimer.nanosNow();
            s.rangeSum(from, from + WINDOW);
            rsum.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("len", Integer.toString(s.size()));
        h.meta("subms.workload.feature", "scalar-f64");
    }
}
