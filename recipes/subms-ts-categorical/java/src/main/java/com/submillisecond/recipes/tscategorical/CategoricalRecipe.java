package com.submillisecond.recipes.tscategorical;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Times the two hot-path optimizer ops: an interner {@code intern} (one hash
 * probe, occasional insert) over a stream of mostly-duplicate symbols, and a
 * column {@code encode} (build the dictionary + code array) over a freshly
 * grown string series. Stages mirror the Rust recipe: {@code intern},
 * {@code encode}.
 */
public final class CategoricalRecipe implements SubMsRecipe {

    private static final String[] ALPHABET = {
            "AAPL", "MSFT", "GOOG", "AMZN", "NVDA", "META", "TSLA", "NFLX"
    };

    private static final int COLUMN_POINTS = 1_024;

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
        return "subms-ts-categorical";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        Lcg rng = new Lcg(params.seed());

        long sink = 0;

        SubMsPerfHarness.Stage sIntern = h.stage("intern", rounds).withKind(SubMsStageKind.HOT_PATH);
        TsStringInterner interner = new TsStringInterner(ALPHABET.length);
        for (int r = 0; r < rounds; r++) {
            String s = ALPHABET[Math.floorMod(rng.nextU32(), ALPHABET.length)];
            long t0 = SubMsTimer.nanosNow();
            int id = interner.intern(s);
            sIntern.record(SubMsTimer.nanosNow() - t0);
            sink += id;
        }

        SubMsPerfHarness.Stage sEncode = h.stage("encode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            TsSeries<String> series = TsSeries.withCapacity(COLUMN_POINTS);
            for (int i = 0; i < COLUMN_POINTS; i++) {
                series.push(i, ALPHABET[Math.floorMod(rng.nextU32(), ALPHABET.length)]);
            }
            long t0 = SubMsTimer.nanosNow();
            TsDictColumn col = TsDictColumn.encode(series);
            sEncode.record(SubMsTimer.nanosNow() - t0);
            sink += col.cardinality();
        }
        BLACK_HOLE = sink;

        h.meta("alphabet_size", Integer.toString(ALPHABET.length));
        h.meta("column_points", Integer.toString(COLUMN_POINTS));
        h.meta("subms.workload.feature", "categorical");
    }

    static volatile long BLACK_HOLE;
}
