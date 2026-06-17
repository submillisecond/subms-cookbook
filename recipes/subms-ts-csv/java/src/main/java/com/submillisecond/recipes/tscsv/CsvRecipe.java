package com.submillisecond.recipes.tscsv;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsDataFrame;

/**
 * Drives a representative subms-ts-csv workload: parse a fixed 4,096-row,
 * 5-column CSV block into a {@link TsDataFrame} ({@code read}) and emit it back
 * to text ({@code write}). Throughput-contracted - a whole-frame parse is
 * O(rows) and runs in milliseconds, not microseconds. Stages mirror the Rust
 * recipe: {@code read}, {@code write}.
 */
public final class CsvRecipe implements SubMsRecipe {

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

    private static String buildCsv(long seed) {
        Lcg rng = new Lcg(seed);
        StringBuilder s = new StringBuilder(ROWS * 24);
        s.append("t,count,price,ok,tag\n");
        for (int i = 0; i < ROWS; i++) {
            long count = (rng.nextU32() & 0xffffffffL) >>> 16;
            double price = ((rng.nextU32() & 0xffffffffL) >>> 8) / 256.0;
            boolean ok = (rng.nextU32() & 1) == 1;
            s.append(i).append(',')
                    .append(count).append(',')
                    .append(String.format("%.3f", price)).append(',')
                    .append(ok ? "true" : "false").append(',')
                    .append(ok ? "up" : "dn").append('\n');
        }
        return s.toString();
    }

    @Override
    public String name() {
        return "subms-ts-csv";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        String text = buildCsv(params.seed());
        TsCsvOptions withTs = TsCsvOptions.defaults().tsColumn("t");

        long sink = 0;

        SubMsPerfHarness.Stage sRead = h.stage("read", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsDataFrame df = TsCsv.readCsv(text, withTs);
            sRead.record(SubMsTimer.nanosNow() - t0);
            sink += df.ncols();
        }

        TsDataFrame df = TsCsv.readCsv(text, TsCsvOptions.defaults());
        SubMsPerfHarness.Stage sWrite = h.stage("write", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            String out = TsCsv.writeCsv(df);
            sWrite.record(SubMsTimer.nanosNow() - t0);
            sink += out.length();
        }
        BLACK_HOLE = sink;

        h.meta("rows", Integer.toString(ROWS));
        h.meta("subms.workload.feature", "csv");
    }

    static volatile long BLACK_HOLE;
}
