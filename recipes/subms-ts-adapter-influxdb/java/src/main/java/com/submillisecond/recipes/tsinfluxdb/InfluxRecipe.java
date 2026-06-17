package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/**
 * Drives a representative subms-ts-adapter-influxdb workload. Two pure stages mirror the
 * Rust recipe: {@code encode} builds the line-protocol batch for a tagged
 * series; {@code decode} parses an annotated-CSV Flux response back into a
 * collection. The network round trip is not benched (it is network-bound and
 * reported, not claimed). Throughput-contracted.
 */
public final class InfluxRecipe implements SubMsRecipe {

    private static final int POINTS = 4_096;

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

    private static String buildCsv(TsSeriesD series) {
        StringBuilder out = new StringBuilder(
                "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n"
                + ",result,table,_time,_value,_field,_measurement,host\n");
        series.toList().forEach(p -> out
                .append(",_result,0,")
                .append(Rfc3339.formatNanos(p.ts()))
                .append(',')
                .append(p.value())
                .append(",v,cpu,edge-01\n"));
        return out.toString();
    }

    @Override
    public String name() {
        return "subms-ts-adapter-influxdb";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long sink = 0;
        TsSeriesD series = buildSeries(params.seed());

        SubMsPerfHarness.Stage sEncode =
                h.stage("encode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            String body = LineProtocol.encodeSeries(series, "");
            sEncode.record(SubMsTimer.nanosNow() - t0);
            sink += body.length();
        }

        String csv = buildCsv(series);
        SubMsPerfHarness.Stage sDecode =
                h.stage("decode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsCollection<Double> coll = FluxCsv.decodeResponse(csv);
            sDecode.record(SubMsTimer.nanosNow() - t0);
            sink += coll.size();
        }
        BLACK_HOLE = sink;

        h.meta("points", Integer.toString(POINTS));
        h.meta("subms.workload.feature", "influxdb");
    }

    static volatile long BLACK_HOLE;
}
