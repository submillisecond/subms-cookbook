package com.submillisecond.recipes.tsmongodb;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.util.ArrayList;
import java.util.List;
import org.bson.Document;

/**
 * Drives a representative subms-ts-adapter-mongodb workload. Two pure stages mirror the
 * Rust recipe: {@code encode} maps a tagged series to its point documents and
 * serialises each to BSON; {@code decode} deserialises and rebuilds the series.
 * The network round trip (the driver) is not benched - it is network-bound and
 * reported, not claimed. Throughput-contracted.
 */
public final class MongoRecipe implements SubMsRecipe {

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

    @Override
    public String name() {
        return "subms-ts-adapter-mongodb";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long sink = 0;
        TsSeriesD series = buildSeries(params.seed());
        // The per-op primitive a tick loop runs: encode / decode ONE point
        // document. That is the asserted sub-ms claim. Whole-batch throughput is
        // reported in the writeup, not benched as a single op.
        List<long[]> pts = new ArrayList<>(series.size());
        series.toList().forEach(p -> pts.add(new long[] {p.ts(), Double.doubleToLongBits(p.value())}));

        SubMsPerfHarness.Stage sEncode =
                h.stage("encode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long[] p = pts.get(r % pts.size());
            long t0 = SubMsTimer.nanosNow();
            byte[] bytes = BsonMapping.docToBytes(BsonMapping.pointDoc(1, p[0], Double.longBitsToDouble(p[1])));
            sEncode.record(SubMsTimer.nanosNow() - t0);
            sink += bytes.length;
        }

        List<byte[]> encoded = new ArrayList<>(pts.size());
        for (long[] p : pts) {
            encoded.add(BsonMapping.docToBytes(BsonMapping.pointDoc(1, p[0], Double.longBitsToDouble(p[1]))));
        }

        SubMsPerfHarness.Stage sDecode =
                h.stage("decode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            byte[] b = encoded.get(r % encoded.size());
            long t0 = SubMsTimer.nanosNow();
            Document d = BsonMapping.docFromBytes(b);
            sink += BsonMapping.pointFromDoc(d).ts();
            sDecode.record(SubMsTimer.nanosNow() - t0);
        }
        BLACK_HOLE = sink;

        h.meta("points", Integer.toString(POINTS));
        h.meta("subms.workload.feature", "mongodb");
    }

    static volatile long BLACK_HOLE;
}
