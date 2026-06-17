package com.submillisecond.recipes.tsgroupby;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.tsexpr.TsExpr;

/**
 * {@code SubMsRecipe} impl - a representative group-by-aggregate workload. The
 * frame is 4,096 rows keyed by a low-cardinality {@code venue} STRING (8
 * distinct symbols), with {@code size} and {@code price} f64 columns. Two stages
 * mirror the Rust recipe: {@code group_agg} runs the full partition + three
 * aggregations (sum, mean, count) per group; {@code value_counts} runs the
 * single-column count path. Throughput-contracted: each timed sample is a full
 * group-by over the whole frame, not a single op.
 */
public final class GroupByRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;
    private static final long CARDINALITY = 8;

    private static final String[] VENUES = {
        "ARCA", "BATS", "EDGX", "IEX", "NSDQ", "NYSE", "PHLX", "XCBO"
    };

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

    private static TsDataFrame buildFrame(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeries<String> venue = TsSeries.withCapacity(ROWS);
        TsSeriesD size = TsSeriesD.withCapacity(ROWS);
        TsSeriesD price = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            String v = VENUES[(int) (rng.nextU32() % CARDINALITY)];
            double s = (double) (rng.nextU32() >>> 18) + 1.0;
            double p = (rng.nextU32() >>> 16) / 100.0;
            venue.push(i, v);
            size.push(i, s);
            price.push(i, p);
        }
        return new TsDataFrame()
                .withColumn("venue", new TsColumn.Str(venue))
                .withColumn("size", new TsColumn.F64(size))
                .withColumn("price", new TsColumn.F64(price));
    }

    @Override
    public String name() {
        return "subms-ts-groupby";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        TsDataFrame frame = buildFrame(params.seed());

        long sink = 0;

        SubMsPerfHarness.Stage sAgg =
                h.stage("group_agg", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsGroupResult result = GroupBy.groupBy(frame, "venue").agg(
                    new TsGroupBy.Agg("total_size", TsExpr.col("size").sum()),
                    new TsGroupBy.Agg("mean_price", TsExpr.col("price").mean()),
                    new TsGroupBy.Agg("n", TsExpr.col("size").count()));
            sAgg.record(SubMsTimer.nanosNow() - t0);
            sink += result.nrows();
        }

        SubMsPerfHarness.Stage sVc =
                h.stage("value_counts", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsGroupResult vc = GroupBy.valueCounts(frame, "venue");
            sVc.record(SubMsTimer.nanosNow() - t0);
            sink += vc.nrows();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("key_cardinality", Long.toString(CARDINALITY));
        h.meta("key_type", "str");
        h.meta("subms.workload.feature", "group-by-aggregate");
    }

    static volatile long BLACK_HOLE;
}
