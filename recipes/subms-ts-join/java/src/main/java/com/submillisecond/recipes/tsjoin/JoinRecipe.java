package com.submillisecond.recipes.tsjoin;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;

/**
 * {@code SubMsRecipe} impl - join two 4,096-row frames on a shared STRING
 * {@code sym} key. Two stages mirror the Rust recipe: {@code hash_inner} runs an
 * inner hash join (the common case); {@code hash_outer} runs a full outer hash
 * join (every left + right row, missing cells filled via validity).
 * Throughput-contracted: each timed sample is a whole-frame join, not a single
 * probe.
 */
public final class JoinRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;

    // Mirrors subms::SubMsLcg (incl. the seed | 1 guard) so the workload's
    // pseudo-random draws match the Rust recipe's drive sequence.
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed | 1L;
        }

        long nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (state >>> 32) & 0xffffffffL;
        }
    }

    // Two frames that overlap on about half their string key space, so an inner
    // join keeps roughly half the rows and an outer join emits the unmatched
    // remainder on both sides - a realistic one-to-one-ish join keyed on a
    // symbol string, not a degenerate all-match.
    private static TsDataFrame[] buildFrames(long seed) {
        Lcg rng = new Lcg(seed);

        TsSeries<String> symL = TsSeries.withCapacity(ROWS);
        TsSeriesD px = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            symL.push(i, String.format("S%06d", i));
            px.push(i, (double) (rng.nextU32() >>> 16));
        }
        TsDataFrame left = new TsDataFrame()
                .withColumn("sym", new TsColumn.Str(symL))
                .withColumn("px", new TsColumn.F64(px));

        TsSeries<String> symR = TsSeries.withCapacity(ROWS);
        TsSeriesD qty = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            // shift the right key space by half so ~half the symbols match.
            symR.push(i, String.format("S%06d", i + ROWS / 2));
            qty.push(i, (double) (rng.nextU32() >>> 16));
        }
        TsDataFrame right = new TsDataFrame()
                .withColumn("sym", new TsColumn.Str(symR))
                .withColumn("qty", new TsColumn.F64(qty));

        return new TsDataFrame[] {left, right};
    }

    @Override
    public String name() {
        return "subms-ts-join";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        TsDataFrame[] frames = buildFrames(params.seed());
        TsDataFrame left = frames[0];
        TsDataFrame right = frames[1];
        String[] keys = {"sym"};

        long sink = 0;

        SubMsPerfHarness.Stage sInner =
                h.stage("hash_inner", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsJoinResult out = TsJoin.hashJoin(left, right, keys, keys, TsJoinKind.INNER);
            sInner.record(SubMsTimer.nanosNow() - t0);
            sink += out.nrows();
        }

        SubMsPerfHarness.Stage sOuter =
                h.stage("hash_outer", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsJoinResult out = TsJoin.hashJoin(left, right, keys, keys, TsJoinKind.OUTER);
            sOuter.record(SubMsTimer.nanosNow() - t0);
            sink += out.nrows();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("subms.workload.feature", "equi-join");
        h.meta("subms.workload.key_type", "str");
    }

    static volatile long BLACK_HOLE;
}
