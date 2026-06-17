package com.submillisecond.recipes.tsexpr;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeriesD;

/**
 * Drives a representative subms-ts-expr workload: evaluate a multi-node
 * expression (a {@code When} over a {@code Compare} with two {@code Binary}
 * arms) over a 4,096-row frame. Two stages mirror the Rust recipe:
 * {@code eval_pipeline} (full per-row array) and {@code eval_agg} (the same
 * tree reduced to a scalar mean). Throughput-contracted: each timed sample is
 * a full whole-frame evaluation, not a single op.
 */
public final class ExprRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;

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

    private static TsDataFrame buildFrame(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeriesD open = TsSeriesD.withCapacity(ROWS);
        TsSeriesD close = TsSeriesD.withCapacity(ROWS);
        for (int i = 0; i < ROWS; i++) {
            double o = (rng.nextU32() & 0xffffffffL) >>> 16;
            double c = (rng.nextU32() & 0xffffffffL) >>> 16;
            open.push(i, o);
            close.push(i, c);
        }
        return new TsDataFrame()
                .withColumn("open", new TsColumn.F64(open))
                .withColumn("close", new TsColumn.F64(close));
    }

    private static TsExpr pipeline() {
        return TsExpr.when(
                TsExpr.col("close").gt(TsExpr.col("open")),
                TsExpr.col("close").sub(TsExpr.col("open")),
                TsExpr.litF64(0.0));
    }

    @Override
    public String name() {
        return "subms-ts-expr";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        TsDataFrame frame = buildFrame(params.seed());
        TsExpr perRow = pipeline();
        TsExpr reduced = pipeline().mean();

        long sink = 0;

        SubMsPerfHarness.Stage sPipe =
                h.stage("eval_pipeline", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsArray arr = Eval.eval(perRow, frame);
            sPipe.record(SubMsTimer.nanosNow() - t0);
            sink += arr.len();
        }

        SubMsPerfHarness.Stage sAgg =
                h.stage("eval_agg", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            var v = Eval.evalScalar(reduced, frame);
            sAgg.record(SubMsTimer.nanosNow() - t0);
            sink += v.hashCode();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("subms.workload.feature", "expr-eval");
    }

    static volatile long BLACK_HOLE;
}
