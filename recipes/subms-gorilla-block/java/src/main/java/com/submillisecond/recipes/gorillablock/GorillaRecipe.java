package com.submillisecond.recipes.gorillablock;

import java.util.List;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsPoint;

/**
 * Drives append (encode), full-block decode, and a windowed range scan over a
 * Gorilla block. Stages mirror the Rust recipe: {@code append}, {@code decode},
 * {@code range_scan}.
 */
public final class GorillaRecipe implements SubMsRecipe {

    // A Gorilla block is a sealed column chunk, not a whole series - real blocks
    // seal at a few thousand points. The decode + scan claims are stated per
    // block at this size; appends are O(1) regardless.
    private static final int BLOCK_POINTS = 1_024;
    private static final long WINDOW = 256L;

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

    private static double gauge(Lcg rng) {
        return 20.0 + ((rng.nextU32() & 0xffffffffL) >>> 28);
    }

    @Override
    public String name() {
        return "subms-gorilla-block";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();
        long baseTs = 1_700_000_000L;

        SubMsPerfHarness.Stage app = h.stage("append", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng = new Lcg(seed);
        TsGorillaBlock blk = TsGorillaBlock.withCapacity(BLOCK_POINTS * 2);
        for (int i = 0; i < entries; i++) {
            if (blk.len() >= BLOCK_POINTS) {
                blk = TsGorillaBlock.withCapacity(BLOCK_POINTS * 2);
            }
            double v = gauge(rng);
            long t0 = SubMsTimer.nanosNow();
            blk.append(baseTs + i, v);
            app.record(SubMsTimer.nanosNow() - t0);
        }

        // A reference block of one sealed chunk, for the read-path stages.
        TsGorillaBlock refb = TsGorillaBlock.withCapacity(BLOCK_POINTS * 2);
        Lcg rng2 = new Lcg(seed ^ 0x55L);
        for (int i = 0; i < BLOCK_POINTS; i++) {
            refb.append(baseTs + i, gauge(rng2));
        }
        byte[] refbytes = refb.bytes();
        int rounds = entries;

        SubMsPerfHarness.Stage dec = h.stage("decode", rounds).withKind(SubMsStageKind.HOT_PATH);
        long sink = 0;
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            List<TsPoint<Double>> pts = TsGorillaBlock.decode(refbytes);
            dec.record(SubMsTimer.nanosNow() - t0);
            sink += pts.size();
        }

        SubMsPerfHarness.Stage rng3stage = h.stage("range_scan", rounds).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng3 = new Lcg(seed ^ 0xdeadL);
        for (int i = 0; i < rounds; i++) {
            long from = baseTs + Math.floorMod((long) rng3.nextU32(), (long) BLOCK_POINTS);
            long t0 = SubMsTimer.nanosNow();
            int c = refb.range(from, from + WINDOW).size();
            rng3stage.record(SubMsTimer.nanosNow() - t0);
            sink += c;
        }

        if (sink == Long.MIN_VALUE) {
            throw new IllegalStateException("unreachable");
        }

        h.meta("block_points", Integer.toString(BLOCK_POINTS));
        h.meta("block_bytes", Integer.toString(refbytes.length));
        h.meta("bytes_per_point", String.format("%.3f", refbytes.length / (double) BLOCK_POINTS));
        h.meta("subms.workload.feature", "scalar-f64");
    }
}
