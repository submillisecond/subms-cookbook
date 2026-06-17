package com.submillisecond.recipes.tsparquet;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/**
 * Drives a representative subms-ts-adapter-parquet workload. {@code encode} persists a
 * series to Parquet bytes; {@code decode} reads it back. Parquet does real work
 * (row groups, column chunks, page headers, a thrift footer), so the asserted
 * workload is a modest series where the whole round trip still clears sub-ms.
 */
public final class ParquetRecipe implements SubMsRecipe {

    private static final int POINTS = 256;

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

    private static TsSeriesD buildSeries(long seed) {
        Lcg rng = new Lcg(seed);
        TsSeriesMetadata meta = new TsSeriesMetadata(1, "cpu")
                .withTag("host", "edge-01")
                .withTag("region", "us-east-1");
        TsSeriesD s = TsSeriesD.withCapacity(POINTS);
        long base = 1_780_000_000_000_000_000L;
        for (int i = 0; i < POINTS; i++) {
            double v = (rng.nextU32() >>> 12) / 1000.0;
            s.push(base + (long) i * 1_000_000_000L, v);
        }
        return s.withMetadata(meta);
    }

    @Override
    public String name() {
        return "subms-ts-adapter-parquet";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long sink = 0;
        TsSeriesD series = buildSeries(params.seed());

        SubMsPerfHarness.Stage sEnc =
                h.stage("encode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            byte[] bytes = ParquetConvert.seriesToParquet(series);
            sEnc.record(SubMsTimer.nanosNow() - t0);
            sink += bytes.length;
        }

        byte[] bytes = ParquetConvert.seriesToParquet(series);
        SubMsPerfHarness.Stage sDec =
                h.stage("decode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsSeriesD s = ParquetConvert.parquetToSeries(bytes);
            sDec.record(SubMsTimer.nanosNow() - t0);
            sink += s.size();
        }
        BLACK_HOLE = sink;

        h.meta("points", Integer.toString(POINTS));
        h.meta("subms.workload.feature", "parquet");
    }

    static volatile long BLACK_HOLE;
}
