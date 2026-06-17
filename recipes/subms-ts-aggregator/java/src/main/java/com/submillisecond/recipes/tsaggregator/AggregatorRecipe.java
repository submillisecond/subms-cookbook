package com.submillisecond.recipes.tsaggregator;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives a representative subms-ts-aggregator workload: streaming ingest
 * (push), O(1) reads of the rolling aggregates (query), and folding two
 * partition windows (merge). Stages mirror the Rust recipe: {@code push},
 * {@code query}, {@code merge}.
 */
public final class AggregatorRecipe implements SubMsRecipe {

    private static final long WINDOW_NS = 1_024L;
    private static final int MERGE_ROUNDS = 2_000;

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
        return "subms-ts-aggregator";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();

        TsWindowedAggregator agg = new TsWindowedAggregator(WINDOW_NS);
        SubMsPerfHarness.Stage push = h.stage("push", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng = new Lcg(seed);
        for (int i = 0; i < entries; i++) {
            double v = ((rng.nextU32() & 0xffffffffL) >>> 8) / 65_536.0;
            long t0 = SubMsTimer.nanosNow();
            agg.push(i, v);
            push.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage query = h.stage("query", entries).withKind(SubMsStageKind.HOT_PATH);
        double sink = 0.0;
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            double r = agg.min().orElse(0.0)
                    + agg.max().orElse(0.0)
                    + agg.sum()
                    + agg.mean().orElse(0.0);
            query.record(SubMsTimer.nanosNow() - t0);
            sink += r;
        }
        BLACK_HOLE = sink;

        TsWindowedAggregator left = new TsWindowedAggregator(WINDOW_NS);
        TsWindowedAggregator right = new TsWindowedAggregator(WINDOW_NS);
        Lcg rng2 = new Lcg(seed ^ 0x55L);
        for (long i = 0; i < WINDOW_NS; i++) {
            left.push(i, (rng2.nextU32() & 0xffffffffL) >>> 16);
            right.push(i, (rng2.nextU32() & 0xffffffffL) >>> 16);
        }
        SubMsPerfHarness.Stage merge = h.stage("merge", MERGE_ROUNDS).withKind(SubMsStageKind.BATCH_OP);
        long countSink = 0;
        for (int i = 0; i < MERGE_ROUNDS; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsWindowedAggregator m = left.merge(right);
            merge.record(SubMsTimer.nanosNow() - t0);
            countSink += m.count();
        }
        BLACK_HOLE_L = countSink;

        h.meta("window_ns", Long.toString(WINDOW_NS));
        h.meta("subms.workload.feature", "rolling-window");
    }

    static volatile double BLACK_HOLE;
    static volatile long BLACK_HOLE_L;
}
