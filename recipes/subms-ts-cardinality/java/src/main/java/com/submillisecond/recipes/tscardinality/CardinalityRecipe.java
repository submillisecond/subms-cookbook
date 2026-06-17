package com.submillisecond.recipes.tscardinality;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Times the two hot-path admission decisions: a guard {@code admit} (counter
 * compare + bump) and a dedup {@code is_new} (one hash set probe + insert).
 * Both are O(1); stages mirror the Rust recipe: {@code admit}, {@code dedup}.
 */
public final class CardinalityRecipe implements SubMsRecipe {

    private static final int CAP = 8_192;

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
        return "subms-ts-cardinality";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        Lcg rng = new Lcg(params.seed());

        long sink = 0;

        SubMsPerfHarness.Stage sAdmit = h.stage("admit", rounds).withKind(SubMsStageKind.HOT_PATH);
        TsCardinalityGuard guard = new TsCardinalityGuard(CAP, TsOverflowPolicy.ALLOW);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            guard.admit();
            sAdmit.record(SubMsTimer.nanosNow() - t0);
            sink += guard.count();
            if (guard.count() >= CAP) guard.release();
        }

        SubMsPerfHarness.Stage sDedup = h.stage("dedup", rounds).withKind(SubMsStageKind.HOT_PATH);
        TsDedupFilter filter = new TsDedupFilter(rounds);
        long seq = 0;
        for (int i = 0; i < rounds; i++) {
            long series = (rng.nextU32() & 0xffffffffL) % 256;
            if (i % 2 == 0) seq++;
            TsIngestKey key = new TsIngestKey(series, seq);
            long t0 = SubMsTimer.nanosNow();
            boolean fresh = filter.isNew(key);
            sDedup.record(SubMsTimer.nanosNow() - t0);
            sink += fresh ? 1 : 0;
        }
        BLACK_HOLE = sink;

        h.meta("cardinality_cap", Integer.toString(CAP));
        h.meta("subms.workload.feature", "cardinality");
    }

    static volatile long BLACK_HOLE;
}
