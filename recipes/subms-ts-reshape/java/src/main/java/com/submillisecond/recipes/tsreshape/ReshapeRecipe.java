package com.submillisecond.recipes.tsreshape;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;

/**
 * {@code SubMsRecipe} impl - reshape a 4,096-row frame two ways, mirroring the
 * Rust recipe. {@code pivot} runs a long-to-wide pivot of an (index, STRING
 * category, value) frame into a roughly 256-row by 16-column grid; {@code melt}
 * unpivots a wide (id, v0..v3) frame into the long form with the {@code Str}
 * {@code variable} column. Throughput-contracted: each timed sample is a
 * whole-frame reshape.
 */
public final class ReshapeRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;
    private static final long INDEX_CARD = 256;
    private static final long CATEGORY_CARD = 16;

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

    // A long-form frame: index in [0, 256), a STRING category in {"c00".."c15"},
    // a random value. The category is a real string column, not a numeric stand-in.
    private static TsDataFrame buildLong(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeriesL index = TsSeriesL.withCapacity(ROWS);
        TsSeries<String> category = TsSeries.withCapacity(ROWS);
        TsSeriesD value = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            long idx = rng.nextU32() % INDEX_CARD;
            String cat = String.format("c%02d", rng.nextU32() % CATEGORY_CARD);
            double val = rng.nextU32() >>> 16;
            index.push(i, idx);
            category.push(i, cat);
            value.push(i, val);
        }
        return new TsDataFrame()
                .withColumn("index", new TsColumn.I64(index))
                .withColumn("category", new TsColumn.Str(category))
                .withColumn("value", new TsColumn.F64(value));
    }

    // A wide frame: an i64 id plus four f64 value columns. Melting to long form
    // emits ROWS * 4 rows, each carrying the source column name in a Str column.
    private static TsDataFrame buildWide(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeriesL id = TsSeriesL.withCapacity(ROWS);
        TsSeriesD v0 = TsSeriesD.withCapacity(ROWS);
        TsSeriesD v1 = TsSeriesD.withCapacity(ROWS);
        TsSeriesD v2 = TsSeriesD.withCapacity(ROWS);
        TsSeriesD v3 = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            id.push(i, i);
            v0.push(i, (double) (rng.nextU32() >>> 16));
            v1.push(i, (double) (rng.nextU32() >>> 16));
            v2.push(i, (double) (rng.nextU32() >>> 16));
            v3.push(i, (double) (rng.nextU32() >>> 16));
        }
        return new TsDataFrame()
                .withColumn("id", new TsColumn.I64(id))
                .withColumn("v0", new TsColumn.F64(v0))
                .withColumn("v1", new TsColumn.F64(v1))
                .withColumn("v2", new TsColumn.F64(v2))
                .withColumn("v3", new TsColumn.F64(v3));
    }

    @Override
    public String name() {
        return "subms-ts-reshape";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        TsDataFrame longFrame = buildLong(params.seed());
        TsDataFrame wide = buildWide(params.seed());
        String[] ids = {"id"};
        String[] vals = {"v0", "v1", "v2", "v3"};

        long sink = 0;

        SubMsPerfHarness.Stage sPivot =
                h.stage("pivot", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsReshapeResult out =
                    TsReshape.pivot(longFrame, "index", "category", "value", PivotAgg.SUM);
            sPivot.record(SubMsTimer.nanosNow() - t0);
            sink += out.nrows();
        }

        SubMsPerfHarness.Stage sMelt =
                h.stage("melt", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsReshapeResult out = TsReshape.melt(wide, ids, vals);
            sMelt.record(SubMsTimer.nanosNow() - t0);
            sink += out.nrows();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("subms.workload.feature", "reshape");
        h.meta("subms.workload.category_type", "str");
    }

    static volatile long BLACK_HOLE;
}
