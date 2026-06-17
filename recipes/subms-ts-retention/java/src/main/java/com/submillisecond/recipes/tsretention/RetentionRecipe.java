package com.submillisecond.recipes.tsretention;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeriesD;

/**
 * Drives a representative subms-ts-retention workload: prune a freshly grown
 * 4,096-point series, once under an age policy (keep the newest ~half) and once
 * under a count policy (keep the newest 1,024). Stages mirror the Rust recipe:
 * {@code apply_age}, {@code apply_count}.
 */
public final class RetentionRecipe implements SubMsRecipe {

    private static final int SERIES = 4_096;

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
        return "subms-ts-retention";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long seed = params.seed();
        Lcg rng = new Lcg(seed);

        TsRetentionPolicy byAge = TsRetentionPolicy.create().maxAgeNs(SERIES / 2);
        TsRetentionPolicy byCount = TsRetentionPolicy.create().maxPoints(1_024);

        long sink = 0;

        SubMsPerfHarness.Stage sAge = h.stage("apply_age", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            TsSeriesD s = TsSeriesD.withCapacity(SERIES);
            for (int i = 0; i < SERIES; i++) {
                s.push(i, (double) ((rng.nextU32() & 0xffffffffL) >>> 16));
            }
            long t0 = SubMsTimer.nanosNow();
            int removed = byAge.apply(s);
            sAge.record(SubMsTimer.nanosNow() - t0);
            sink += removed;
        }

        SubMsPerfHarness.Stage sCnt = h.stage("apply_count", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            TsSeriesD s = TsSeriesD.withCapacity(SERIES);
            for (int i = 0; i < SERIES; i++) {
                s.push(i, (double) ((rng.nextU32() & 0xffffffffL) >>> 16));
            }
            long t0 = SubMsTimer.nanosNow();
            int removed = byCount.apply(s);
            sCnt.record(SubMsTimer.nanosNow() - t0);
            sink += removed;
        }
        BLACK_HOLE = sink;

        h.meta("series_points", Integer.toString(SERIES));
        h.meta("subms.workload.feature", "retention");
    }

    static volatile long BLACK_HOLE;
}
