package com.submillisecond.recipes.events;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stages: build / emit_sync / emit_async. Mirrors the Rust recipe. */
public final class EventsRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "subms-events";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();

        SubMsPerfHarness.Stage sBuild = h.stage("build", entries).withKind(SubMsStageKind.BATCH_OP);
        long sink = 0;
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            Event e = Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN");
            sBuild.record(SubMsTimer.nanosNow() - t0);
            sink += e.topic().length();
        }

        Event ev = Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN");

        EventDispatcher syncBus = EventDispatcher.sync();
        syncBus.addListener(e -> {});
        SubMsPerfHarness.Stage sSync = h.stage("emit_sync", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            syncBus.emit(ev);
            sSync.record(SubMsTimer.nanosNow() - t0);
        }

        EventDispatcher asyncBus = EventDispatcher.asynchronous();
        asyncBus.addListener(e -> {});
        SubMsPerfHarness.Stage sAsync = h.stage("emit_async", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            asyncBus.emit(ev);
            sAsync.record(SubMsTimer.nanosNow() - t0);
        }
        asyncBus.stop();

        if (sink == Long.MIN_VALUE) {
            throw new IllegalStateException("unreachable");
        }
        h.meta("subms.workload.feature", "in-process-dispatch");
    }
}
