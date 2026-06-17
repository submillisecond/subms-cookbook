package com.submillisecond.recipes.timer.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MeteredTimerWheelTest {

    @Test
    void scheduledCounterIncrementsOnSchedule() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        w.schedule(1, 1);
        w.schedule(2, 2);
        assertEquals(2L, w.metrics().scheduled);
    }

    @Test
    void firedCounterMatchesTickResults() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        w.schedule(1, 1);
        w.schedule(2, 2);
        w.schedule(2, 3);
        int seen = 0;
        for (int i = 0; i < 4; i++) seen += w.tick().size();
        assertEquals(3, seen);
        assertEquals(3L, w.metrics().fired);
    }

    @Test
    void cancelledCounterOnlyIncrementsOnRealCancel() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        long id = w.schedule(5, 1);
        assertTrue(w.cancel(id));
        assertFalse(w.cancel(9999L));
        assertEquals(1L, w.metrics().cancelled);
    }

    @Test
    void ticksCounterIncrementsPerTick() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        for (int i = 0; i < 7; i++) w.tick();
        assertEquals(7L, w.metrics().ticks);
    }

    @Test
    void cascadeEventsStaysZeroOnSingleLevelWheel() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        for (int d = 1; d <= 20; d++) w.schedule(d, d);
        for (int i = 0; i < 30; i++) w.tick();
        assertEquals(0L, w.metrics().cascadeEvents);
    }

    @Test
    void metricsSnapshotIndependentOfWheelState() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        w.schedule(1, 1);
        TimerMetrics snapA = w.metrics();
        w.schedule(2, 2);
        TimerMetrics snapB = w.metrics();
        assertEquals(1L, snapA.scheduled);
        assertEquals(2L, snapB.scheduled);
    }

    @Test
    void scheduleFireCancelFullLifecycleCounters() {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(64);
        w.schedule(2, 1);
        long b = w.schedule(2, 2);
        w.cancel(b);
        List<Integer> total = new ArrayList<>();
        total.addAll(w.tick());
        total.addAll(w.tick());
        assertEquals(List.of(1), total);
        TimerMetrics m = w.metrics();
        assertEquals(2L, m.scheduled);
        assertEquals(1L, m.cancelled);
        assertEquals(1L, m.fired);
        assertEquals(2L, m.ticks);
    }
}
