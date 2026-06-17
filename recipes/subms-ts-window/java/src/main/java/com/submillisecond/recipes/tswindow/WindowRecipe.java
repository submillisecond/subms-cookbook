package com.submillisecond.recipes.tswindow;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.tsexpr.TsArray;
import com.submillisecond.recipes.tsexpr.TsExpr;

/**
 * {@code SubMsRecipe} impl - run three representative window passes over a
 * STRING-partitioned 4,096-row {@link TsDataFrame}: {@code lag} (shift),
 * {@code cumsum} (running reduction), and {@code over} (per-partition aggregate
 * broadcast). The frame is partitioned by a low-cardinality {@code symbol}
 * STRING (16 venues) so each pass does a realistic typed-key group-sort-scan,
 * not a degenerate single-partition or all-singleton case.
 * Throughput-contracted: each timed sample is a full whole-frame window pass,
 * and {@code over} (per-partition sub-frame + expr eval) is the heavy stage.
 */
public final class WindowRecipe implements SubMsRecipe {

    private static final int ROWS = 4_096;
    private static final long PARTITIONS = 16L;

    private static final String[] SYMBOLS = {
        "ARCA", "BATS", "EDGX", "IEX", "NSDQ", "NYSE", "PHLX", "XCBO", "LSE", "TSE", "HKEX", "SGX",
        "ASX", "BMV", "JSE", "B3"
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
        TsSeries<String> symbol = new TsSeries<>();
        TsSeriesD val = new TsSeriesD();
        for (int i = 0; i < ROWS; i++) {
            String s = SYMBOLS[(int) (rng.nextU32() % PARTITIONS)];
            double v = rng.nextU32() >>> 16;
            symbol.push(i, s);
            val.push(i, v);
        }
        TsDataFrame f = new TsDataFrame();
        f.pushColumn("symbol", new TsColumn.Str(symbol));
        f.pushColumn("val", new TsColumn.F64(val));
        return f;
    }

    @Override
    public String name() {
        return "subms-ts-window";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        TsDataFrame frame = buildFrame(params.seed());
        String[] keys = {"symbol"};
        TsExpr agg = TsExpr.col("val").mean();

        long sink = 0;

        SubMsPerfHarness.Stage sLag = h.stage("lag", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsArray arr = TsWindow.lag(frame, "val", 1, keys);
            sLag.record(SubMsTimer.nanosNow() - t0);
            sink += arr.len();
        }

        SubMsPerfHarness.Stage sCum = h.stage("cumsum", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsArray arr = TsWindow.cumsum(frame, "val", keys, null);
            sCum.record(SubMsTimer.nanosNow() - t0);
            sink += arr.len();
        }

        SubMsPerfHarness.Stage sOver = h.stage("over", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsArray arr = TsWindow.over(frame, agg, keys);
            sOver.record(SubMsTimer.nanosNow() - t0);
            sink += arr.len();
        }
        BLACK_HOLE = sink;

        h.meta("frame_rows", Integer.toString(ROWS));
        h.meta("partitions", Long.toString(PARTITIONS));
        h.meta("subms.workload.feature", "window-functions");
    }

    static volatile long BLACK_HOLE;
}
