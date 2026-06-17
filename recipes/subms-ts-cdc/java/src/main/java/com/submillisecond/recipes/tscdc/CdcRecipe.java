package com.submillisecond.recipes.tscdc;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/**
 * Drives the CDC hot paths. {@code push_notify}: append a point to a one-series
 * collection with a single live 4096-cap subscriber, timing push + fan-out
 * together. {@code recv}: time a single {@code tryRecv} off a pre-filled ring.
 * The ring is drained between push batches so it never saturates and the drop
 * path stays cold. Stages mirror the Rust recipe.
 */
public final class CdcRecipe implements SubMsRecipe {

    private static final int RING_CAP = 4_096;
    private static final int DRAIN_EVERY = 2_048;

    @Override
    public String name() {
        return "subms-ts-cdc";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();

        TsObservableCollection<Double> obs = new TsObservableCollection<>();
        TsSubscription<Double> sub = obs.subscribe(RING_CAP);
        long id = obs.register(TsSeriesMetadata.of(1L, "bench"));

        SubMsPerfHarness.Stage sPush = h.stage("push_notify", rounds).withKind(SubMsStageKind.HOT_PATH);
        long ts = 0L;
        for (int i = 0; i < rounds; i++) {
            ts++;
            long t0 = SubMsTimer.nanosNow();
            obs.push(id, ts, (double) ts);
            sPush.record(SubMsTimer.nanosNow() - t0);
            if (i % DRAIN_EVERY == DRAIN_EVERY - 1) {
                while (sub.tryRecv() != null) { /* drain */ }
            }
        }
        while (sub.tryRecv() != null) { /* drain */ }

        int recvRounds = Math.min(rounds, RING_CAP - 1);
        for (int i = 0; i < recvRounds; i++) {
            ts++;
            obs.push(id, ts, (double) ts);
        }
        SubMsPerfHarness.Stage sRecv = h.stage("recv", recvRounds).withKind(SubMsStageKind.HOT_PATH);
        long sink = 0;
        for (int i = 0; i < recvRounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            TsChangeEvent<Double> ev = sub.tryRecv();
            sRecv.record(SubMsTimer.nanosNow() - t0);
            if (ev != null) sink++;
        }
        BLACK_HOLE = sink;

        h.meta("ring_capacity", Integer.toString(RING_CAP));
        h.meta("dropped_events", Long.toString(obs.droppedEvents()));
        h.meta("subms.workload.feature", "cdc-publish-recv");
    }

    static volatile long BLACK_HOLE;
}
