package com.submillisecond.recipes.zonemap;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives zone recording (observe) and pruning a 100k-zone index (candidates).
 * Stages mirror the Rust recipe: {@code observe}, {@code candidates}.
 */
public final class ZoneMapRecipe implements SubMsRecipe {

    private static final long INDEX_BLOCKS = 100_000L;
    private static final long WINDOW = 2_500L;

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

    private static TsZone zone(long id, double valueMax) {
        long base = id * 1_000L;
        return new TsZone(id, base, base + 999, 0.0, valueMax, 1_000);
    }

    @Override
    public String name() {
        return "subms-zone-map";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();

        TsZoneMap index = TsZoneMap.withCapacity((int) INDEX_BLOCKS);
        for (long id = 0; id < INDEX_BLOCKS; id++) {
            index.observeZone(zone(id, id % 500));
        }

        SubMsPerfHarness.Stage sObs = h.stage("observe", entries).withKind(SubMsStageKind.HOT_PATH);
        TsZoneMap scratch = TsZoneMap.withCapacity(entries);
        Lcg rng = new Lcg(seed);
        for (int i = 0; i < entries; i++) {
            TsZone z = zone(i, Integer.toUnsignedLong(rng.nextU32()) % 500);
            long t0 = SubMsTimer.nanosNow();
            scratch.observeZone(z);
            sObs.record(SubMsTimer.nanosNow() - t0);
        }

        long span = INDEX_BLOCKS * 1_000L;
        SubMsPerfHarness.Stage sCand = h.stage("candidates", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng2 = new Lcg(seed ^ 0x99L);
        long sink = 0;
        for (int i = 0; i < entries; i++) {
            long lo = Math.floorMod(Integer.toUnsignedLong(rng2.nextU32()), span);
            long t0 = SubMsTimer.nanosNow();
            int c = index.candidates(lo, lo + WINDOW).length;
            sCand.record(SubMsTimer.nanosNow() - t0);
            sink += c;
        }

        if (sink == Long.MIN_VALUE) {
            throw new IllegalStateException("unreachable");
        }

        h.meta("index_blocks", Long.toString(INDEX_BLOCKS));
        h.meta("subms.workload.feature", "time-window-prune");
    }
}
