package com.submillisecond.recipes.eventstore;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.events.Event;

/** Stages: append / replay / catch_up. Mirrors the Rust recipe. */
public final class EventStoreRecipe implements SubMsRecipe {
    private static final int REPLAY_N = 1_000;

    @Override
    public String name() {
        return "subms-events-store";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();

        EventStore store = new EventStore();
        SubMsPerfHarness.Stage sApp = h.stage("append", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            Event ev = Event.builder("evt").at("t").attr("i", Integer.toString(i)).build();
            long t0 = SubMsTimer.nanosNow();
            store.append(ev);
            sApp.record(SubMsTimer.nanosNow() - t0);
        }

        EventStore base = new EventStore();
        for (int i = 0; i < REPLAY_N; i++) {
            base.append(Event.builder("evt").at("t").build());
        }
        long sink = 0;
        SubMsPerfHarness.Stage sRep = h.stage("replay", entries).withKind(SubMsStageKind.BATCH_OP);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            long count = Projector.replay(base, 0L, (n, e) -> n + 1);
            sRep.record(SubMsTimer.nanosNow() - t0);
            sink += count;
        }

        Projector<Long> proj = new Projector<>(0L);
        proj.catchUp(base, (n, e) -> n + 1);
        SubMsPerfHarness.Stage sCu = h.stage("catch_up", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            base.append(Event.builder("x").at("t").build());
            long t0 = SubMsTimer.nanosNow();
            proj.catchUp(base, (n, e) -> n + 1);
            sCu.record(SubMsTimer.nanosNow() - t0);
        }

        if (sink == Long.MIN_VALUE) {
            throw new IllegalStateException("unreachable");
        }
        h.meta("replay_window", Integer.toString(REPLAY_N));
        h.meta("final_log_len", Integer.toString(base.size()));
        h.meta("subms.workload.feature", "in-memory-event-sourcing");
    }
}
