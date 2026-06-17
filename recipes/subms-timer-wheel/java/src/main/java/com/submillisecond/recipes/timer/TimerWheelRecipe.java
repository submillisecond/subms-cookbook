package com.submillisecond.recipes.timer;

import java.util.ArrayList;
import java.util.List;
import java.util.Random;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

public final class TimerWheelRecipe implements SubMsRecipe {
    @Override public String name() { return "timer-wheel"; }
    @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        long seed = params.seed();
        int slots = 1024;
        TimerWheel<Integer> w = new TimerWheel<>(slots);

        Random r0 = new Random(seed);
        for (int i = 0; i < warmup; i++) {
            w.schedule(r0.nextInt(slots * 4), i);
        }

        SubMsPerfHarness.Stage sched = h.stage("schedule", entries).withKind(SubMsStageKind.HOT_PATH);
        Random r1 = new Random(seed + 1);
        List<Long> ids = new ArrayList<>(entries);
        for (int i = 0; i < entries; i++) {
            int delay = r1.nextInt(slots * 4);
            long t0 = SubMsTimer.nanosNow();
            long id = w.schedule(delay, i);
            sched.record(SubMsTimer.nanosNow() - t0);
            ids.add(id);
        }

        SubMsPerfHarness.Stage cancel = h.stage("cancel", entries / 2).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ids.size(); i += 2) {
            long t0 = SubMsTimer.nanosNow();
            w.cancel(ids.get(i));
            cancel.record(SubMsTimer.nanosNow() - t0);
        }

        SubMsPerfHarness.Stage tick = h.stage("tick", slots * 5).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < slots * 5; i++) {
            long t0 = SubMsTimer.nanosNow();
            w.tick();
            tick.record(SubMsTimer.nanosNow() - t0);
        }

        h.meta("slots", Integer.toString(slots));
    }
}
