package com.submillisecond.recipes.tsasofjoin;

import java.util.List;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Drives a representative subms-ts-asof-join workload: a full backward join
 * and a full nearest join over two 1,024-point series, repeated. Stages
 * mirror the Rust recipe: {@code join_backward}, {@code join_nearest}.
 */
public final class AsofJoinRecipe implements SubMsRecipe {

    private static final int SERIES = 1_024;

    // Mirrors subms::SubMsLcg (incl. the seed | 1 guard) so the workload's
    // pseudo-random draws match the Rust recipe's drive sequence.
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

    @Override
    public String name() {
        return "subms-ts-asof-join";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long seed = params.seed();

        Lcg rng = new Lcg(seed);
        TsSeries<Double> left = TsSeries.withCapacity(SERIES);
        TsSeries<Double> right = TsSeries.withCapacity(SERIES);
        for (long i = 0; i < SERIES; i++) {
            left.push(i * 3, (double) ((rng.nextU32() & 0xffffffffL) >>> 16));
            right.push(i * 2, (double) ((rng.nextU32() & 0xffffffffL) >>> 16));
        }

        SubMsPerfHarness.Stage back =
                h.stage("join_backward", rounds).withKind(SubMsStageKind.HOT_PATH);
        long sink = 0;
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            List<AsofJoin.TsMatch> m = AsofJoin.backward(left, right);
            back.record(SubMsTimer.nanosNow() - t0);
            sink += m.size();
        }
        BLACK_HOLE_L = sink;

        SubMsPerfHarness.Stage near =
                h.stage("join_nearest", rounds).withKind(SubMsStageKind.HOT_PATH);
        sink = 0;
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            List<AsofJoin.TsMatch> m = AsofJoin.nearest(left, right, 4);
            near.record(SubMsTimer.nanosNow() - t0);
            sink += m.size();
        }
        BLACK_HOLE_L = sink;

        h.meta("series_points", Integer.toString(SERIES));
        h.meta("subms.workload.feature", "asof-join");
    }

    static volatile long BLACK_HOLE_L;
}
