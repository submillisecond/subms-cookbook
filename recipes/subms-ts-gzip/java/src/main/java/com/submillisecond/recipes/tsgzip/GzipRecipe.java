package com.submillisecond.recipes.tsgzip;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Drives a representative subms-ts-gzip workload: a full gzip+json encode and
 * decode round over a 128-point f64 series. Stages mirror the Rust recipe:
 * {@code encode}, {@code decode}.
 */
public final class GzipRecipe implements SubMsRecipe {

    private static final int SERIES = 128;
    private static final int LEVEL = 6;

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

    @Override
    public String name() {
        return "subms-ts-gzip";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long seed = params.seed();

        Lcg rng = new Lcg(seed);
        TsSeries<Double> s = TsSeries.withCapacity(SERIES);
        long ts = 0L;
        for (int i = 0; i < SERIES; i++) {
            s.push(ts, (double) ((rng.nextU32() & 0xffffffffL) >>> 16) / 7.0);
            ts += 10L + (rng.nextU32() & 0xffffffffL) % 40L;
        }
        TsGzipCodec<Double> codec = new TsGzipCodec<>(new JsonInnerCodec(), LEVEL);
        byte[] encoded = codec.encode(s);

        long sink = 0;
        SubMsPerfHarness.Stage sEnc = h.stage("encode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            byte[] bytes = codec.encode(s);
            sEnc.record(SubMsTimer.nanosNow() - t0);
            sink += bytes.length;
        }

        SubMsPerfHarness.Stage sDec = h.stage("decode", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsSeries<Double> back = codec.decode(encoded);
            sDec.record(SubMsTimer.nanosNow() - t0);
            sink += back.size();
        }
        BLACK_HOLE = sink;

        h.meta("series_points", Integer.toString(SERIES));
        h.meta("gzip_level", Integer.toString(LEVEL));
        h.meta("encoded_bytes", Integer.toString(encoded.length));
        h.meta("subms.workload.feature", "gzip-codec");
    }

    static volatile long BLACK_HOLE;
}
