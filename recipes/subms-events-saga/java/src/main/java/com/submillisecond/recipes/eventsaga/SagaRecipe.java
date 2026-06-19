package com.submillisecond.recipes.eventsaga;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stages: build / commit / compensate. Mirrors the Rust recipe. */
public final class SagaRecipe implements SubMsRecipe {
    private static final int STEPS = 8;

    private static Saga commitSaga() {
        Saga s = new Saga("bench");
        for (int i = 0; i < STEPS; i++) {
            s.step("s" + i, () -> {}, () -> {});
        }
        return s;
    }

    private static Saga failSaga() {
        Saga s = new Saga("bench");
        for (int i = 0; i < STEPS - 1; i++) {
            s.step("s" + i, () -> {}, () -> {});
        }
        s.step("last", () -> {
            throw new RuntimeException("boom");
        }, () -> {});
        return s;
    }

    @Override
    public String name() {
        return "subms-events-saga";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();

        SubMsPerfHarness.Stage sBuild = h.stage("build", entries).withKind(SubMsStageKind.BATCH_OP);
        long sink = 0;
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            Saga saga = commitSaga();
            sBuild.record(SubMsTimer.nanosNow() - t0);
            sink += saga.hashCode();
        }

        Saga saga = commitSaga();
        SubMsPerfHarness.Stage sCommit = h.stage("commit", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            SagaReport r = saga.run();
            sCommit.record(SubMsTimer.nanosNow() - t0);
            sink += r.isCommitted() ? 1 : 0;
        }

        Saga fsaga = failSaga();
        SubMsPerfHarness.Stage sComp = h.stage("compensate", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            SagaReport r = fsaga.run();
            sComp.record(SubMsTimer.nanosNow() - t0);
            sink += r.compensated().size();
        }

        if (sink == Long.MIN_VALUE) {
            throw new IllegalStateException("unreachable");
        }
        h.meta("steps", Integer.toString(STEPS));
        h.meta("subms.workload.feature", "in-process-compensation");
    }
}
