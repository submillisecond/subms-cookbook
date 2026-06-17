package com.submillisecond.recipes.tsfill;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Drives a representative subms-ts-fill workload: a full linear fill and a
 * full LOCF fill over a 1,024-point gappy series, repeated. Stages mirror the
 * Rust recipe: {@code fill_linear}, {@code fill_locf}.
 */
public final class FillRecipe implements SubMsRecipe {

    private static final int SERIES = 1_024;
    private static final long STEP = 10;

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
        return "subms-ts-fill";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long seed = params.seed();

        Lcg rng = new Lcg(seed);
        TsSeries<Double> s = TsSeries.withCapacity(SERIES);
        long ts = 0;
        for (int i = 0; i < SERIES; i++) {
            s.push(ts, (double) ((rng.nextU32() & 0xffffffffL) >>> 16));
            ts += 30 + (rng.nextU32() & 0xffffffffL) % 20;
        }

        SubMsPerfHarness.Stage lin =
                h.stage("fill_linear", rounds).withKind(SubMsStageKind.HOT_PATH);
        long sink = 0;
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsSeries<Double> f = Fill.linear(s, STEP);
            lin.record(SubMsTimer.nanosNow() - t0);
            sink += f.size();
        }
        BLACK_HOLE_L = sink;

        SubMsPerfHarness.Stage locf =
                h.stage("fill_locf", rounds).withKind(SubMsStageKind.HOT_PATH);
        sink = 0;
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsSeries<Double> f = Fill.locf(s, STEP);
            locf.record(SubMsTimer.nanosNow() - t0);
            sink += f.size();
        }
        BLACK_HOLE_L = sink;

        h.meta("series_points", Integer.toString(SERIES));
        h.meta("subms.workload.feature", "gap-fill");
    }

    static volatile long BLACK_HOLE_L;
}
