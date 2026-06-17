package com.submillisecond.recipes.tsdownsampler;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives a representative subms-ts-downsampler workload: streaming ingest
 * (push) into a 3-tier 1s / 1m / 1h pipeline, then full bucket-stats queries
 * across the tiers. Stages mirror the Rust recipe: {@code push},
 * {@code bucket_stats}.
 */
public final class DownsamplerRecipe implements SubMsRecipe {

    private static final long[] TIERS = {1_000_000_000L, 60_000_000_000L, 3_600_000_000_000L};
    private static final long STEP_NS = 100_000_000L;

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
        return "subms-ts-downsampler";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();

        TsDownsampler d = new TsDownsampler(TIERS);
        SubMsPerfHarness.Stage push = h.stage("push", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng = new Lcg(seed);
        long ts = 0L;
        for (int i = 0; i < entries; i++) {
            double v = ((rng.nextU32() & 0xffffffffL) >>> 8) / 65_536.0;
            long t0 = SubMsTimer.nanosNow();
            d.push(ts, v);
            push.record(SubMsTimer.nanosNow() - t0);
            ts += STEP_NS;
        }
        d.flush();

        long span = Math.max(ts, 1L);
        SubMsPerfHarness.Stage query = h.stage("bucket_stats", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng2 = new Lcg(seed ^ 0x33L);
        long sink = 0L;
        for (int i = 0; i < entries; i++) {
            int level = (int) ((rng2.nextU32() & 0xffffffffL) % TIERS.length);
            long at = Math.floorMod(rng2.nextU32() & 0xffffffffL, span);
            long t0 = SubMsTimer.nanosNow();
            var s = d.bucketStats(level, at);
            query.record(SubMsTimer.nanosNow() - t0);
            sink += s.isPresent() ? 1 : 0;
        }
        BLACK_HOLE_L = sink;

        h.meta("tier0_buckets", Integer.toString(d.tier(0).size()));
        h.meta("subms.workload.feature", "tiered-rollup");
    }

    static volatile long BLACK_HOLE_L;
}
