package com.submillisecond.recipes.tdigest;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives streaming ingest (add), quantile queries, and a per-shard merge over
 * a t-digest. Stages mirror the Rust recipe: {@code add}, {@code quantile},
 * {@code merge}.
 */
public final class TDigestRecipe implements SubMsRecipe {

    private static final double COMPRESSION = 100.0;
    private static final int MERGE_ROUNDS = 2_000;

    // Mirrors subms::SubMsLcg so the workload's pseudo-random draws match the
    // Rust recipe's drive sequence.
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed;
        }

        int nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> 32);
        }
    }

    @Override
    public String name() {
        return "subms-tdigest";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();

        TsTDigest d = new TsTDigest(COMPRESSION);
        SubMsPerfHarness.Stage sAdd = h.stage("add", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng = new Lcg(seed);
        for (int i = 0; i < entries; i++) {
            double v = ((rng.nextU32() & 0xffffffffL) >>> 8) / 65_536.0;
            long t0 = SubMsTimer.nanosNow();
            d.add(v);
            sAdd.record(SubMsTimer.nanosNow() - t0);
        }
        d.compact();

        double[] qs = {0.5, 0.9, 0.99, 0.999};
        SubMsPerfHarness.Stage sQ = h.stage("quantile", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng2 = new Lcg(seed ^ 0x77L);
        double sink = 0.0;
        for (int i = 0; i < entries; i++) {
            double q = qs[(int) ((rng2.nextU32() & 0xffffffffL) % qs.length)];
            long t0 = SubMsTimer.nanosNow();
            double v = d.quantile(q);
            sQ.record(SubMsTimer.nanosNow() - t0);
            sink += v;
        }

        TsTDigest a = new TsTDigest(COMPRESSION);
        TsTDigest b = new TsTDigest(COMPRESSION);
        Lcg rng3 = new Lcg(seed ^ 0x55L);
        for (int i = 0; i < 100_000; i++) {
            a.add((rng3.nextU32() & 0xffffffffL) >>> 8);
            b.add((rng3.nextU32() & 0xffffffffL) >>> 8);
        }
        a.compact();
        b.compact();
        SubMsPerfHarness.Stage sM = h.stage("merge", MERGE_ROUNDS).withKind(SubMsStageKind.BATCH_OP);
        for (int i = 0; i < MERGE_ROUNDS; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsTDigest m = a.merge(b);
            sM.record(SubMsTimer.nanosNow() - t0);
            sink += m.count();
        }

        if (sink == Double.NEGATIVE_INFINITY) {
            throw new IllegalStateException("unreachable");
        }

        h.meta("compression", Long.toString((long) COMPRESSION));
        h.meta("serialized_bytes", Integer.toString(d.serialize().length));
        h.meta("subms.workload.feature", "streaming-quantile");
    }
}
