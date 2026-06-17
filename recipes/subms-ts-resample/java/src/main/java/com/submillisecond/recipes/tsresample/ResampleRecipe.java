package com.submillisecond.recipes.tsresample;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Drives a representative subms-ts-resample workload: a full grid resample of
 * a 1,024-point irregular series, once under MEAN and once under LAST. Stages
 * mirror the Rust recipe: {@code resample_mean}, {@code resample_last}.
 */
public final class ResampleRecipe implements SubMsRecipe {

    private static final int SERIES = 1_024;
    private static final long PERIOD = 100L;

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
        return "subms-ts-resample";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long seed = params.seed();

        Lcg rng = new Lcg(seed);
        TsSeries<Double> s = TsSeries.withCapacity(SERIES);
        long ts = 0L;
        for (int i = 0; i < SERIES; i++) {
            s.push(ts, (double) ((rng.nextU32() & 0xffffffffL) >>> 16));
            ts += 10L + (rng.nextU32() & 0xffffffffL) % 40L;
        }

        SubMsPerfHarness.Stage sMean = h.stage("resample_mean", rounds).withKind(SubMsStageKind.HOT_PATH);
        long sink = 0;
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsSeries<Double> g = Resample.toGrid(s, PERIOD, TsResampleMode.MEAN);
            sMean.record(SubMsTimer.nanosNow() - t0);
            sink += g.size();
        }

        SubMsPerfHarness.Stage sLast = h.stage("resample_last", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsSeries<Double> g = Resample.toGrid(s, PERIOD, TsResampleMode.LAST);
            sLast.record(SubMsTimer.nanosNow() - t0);
            sink += g.size();
        }
        BLACK_HOLE = sink;

        h.meta("series_points", Integer.toString(SERIES));
        h.meta("subms.workload.feature", "grid-resample");
    }

    static volatile long BLACK_HOLE;
}
