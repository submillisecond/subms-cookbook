package com.submillisecond.recipes.tslazy;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsplan.TsLatencyCertificate;

/**
 * Drives a representative subms-ts-lazy workload. {@code optimise_collect}
 * builds, optimises, and collects a 5-op pipeline over a 4,096-row frame -
 * throughput-contracted (a generous guard, not a tight p99). {@code certify}
 * lowers the same pipeline to a latency certificate - per-op work over the plan
 * node list, independent of row count, and genuinely sub-ms.
 *
 * <p>Mirrors the Rust LazyRecipe stage-for-stage.
 */
public final class LazyRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;

    // Mirrors subms::SubMsLcg (incl. the seed | 1 guard) so the workload's draws
    // match the Rust recipe's drive sequence.
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

    private static TsDataFrame buildFrame(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeriesD px = TsSeriesD.withCapacity(ROWS);
        TsSeriesL qty = TsSeriesL.withCapacity(ROWS);
        TsSeriesL venue = TsSeriesL.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            double p = (double) (((rng.nextU32() & 0xffffffffL) >>> 16)) / 64.0;
            long q = (rng.nextU32() & 0xffffffffL) >>> 24;
            long v = (rng.nextU32() & 0xffffffffL) % 4;
            px.push(i, p);
            qty.push(i, q);
            venue.push(i, v);
        }
        return new TsDataFrame()
                .withColumn("px", new TsColumn.F64(px))
                .withColumn("qty", new TsColumn.I64(qty))
                .withColumn("venue", new TsColumn.I64(venue));
    }

    private static LazyTsFrame pipeline(TsDataFrame frame) {
        return new LazyTsFrame(frame)
                .filter(TsExpr.col("px").gt(TsExpr.litF64(128.0)))
                .withColumn("notional", TsExpr.col("px").mul(TsExpr.col("qty")))
                .filter(TsExpr.col("venue").eq(TsExpr.litI64(1)))
                .sortBy("notional", false)
                .select("notional", "px");
    }

    @Override
    public String name() {
        return "subms-ts-lazy";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        TsDataFrame frame = buildFrame(params.seed());
        long sink = 0;

        SubMsPerfHarness.Stage sCollect =
                h.stage("optimise_collect", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            ResultFrame result = pipeline(frame).collect();
            sCollect.record(SubMsTimer.nanosNow() - t0);
            sink += result.nrows();
        }

        SubMsPerfHarness.Stage sCertify =
                h.stage("certify", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            LazyTsFrame lazy = pipeline(frame);
            long t0 = SubMsTimer.nanosNow();
            TsLatencyCertificate cert = lazy.certify("ci-dedicated", 0);
            sCertify.record(SubMsTimer.nanosNow() - t0);
            sink += cert.totalP99Ns();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("subms.workload.feature", "lazy-plan-certify");
    }

    static volatile long BLACK_HOLE;
}
