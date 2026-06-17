package com.submillisecond.recipes.tsanomaly;

import java.util.Optional;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives a representative subms-ts-anomaly workload: a streaming push of a
 * jittered baseline with occasional spikes, each scored + admitted. Stage
 * mirrors the Rust recipe: {@code push}.
 */
public final class AnomalyRecipe implements SubMsRecipe {

    private static final long WINDOW_NS = 1_024L;
    private static final double SIGMA = 3.0;

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
        return "subms-ts-anomaly";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        long seed = params.seed();

        TsAnomalyDetector d = new TsAnomalyDetector(WINDOW_NS, SIGMA);
        SubMsPerfHarness.Stage push = h.stage("push", entries).withKind(SubMsStageKind.HOT_PATH);
        Lcg rng = new Lcg(seed);
        long flagged = 0;
        for (int i = 0; i < entries; i++) {
            double base = 100.0 + ((rng.nextU32() & 0xffffffffL) >>> 24) / 256.0;
            double v = (rng.nextU32() & 0xffffffffL) % 5_000L == 0 ? base + 500.0 : base;
            long t0 = SubMsTimer.nanosNow();
            Optional<TsAnomaly> hit = d.push(i, v);
            push.record(SubMsTimer.nanosNow() - t0);
            if (hit.isPresent()) {
                flagged++;
            }
        }

        h.meta("flagged", Long.toString(flagged));
        h.meta("subms.workload.feature", "rolling-zscore");
    }
}
