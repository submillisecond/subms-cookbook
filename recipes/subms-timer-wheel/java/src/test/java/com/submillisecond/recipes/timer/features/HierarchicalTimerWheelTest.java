package com.submillisecond.recipes.timer.features;

import com.submillisecond.recipes.timer.TimerError;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class HierarchicalTimerWheelTest {

    @Test
    void shortDelayFiresOnCorrectTick() {
        HierarchicalTimerWheel<String> w = new HierarchicalTimerWheel<>();
        w.schedule(5, "a");
        for (int i = 0; i < 4; i++) assertTrue(w.tick().isEmpty());
        assertEquals(List.of("a"), w.tick());
    }

    @Test
    void cascadeBoundary64TicksFiresCorrectly() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        w.schedule(64, 7);
        long firedAt = -1;
        for (long i = 1; i <= 70; i++) {
            List<Integer> fired = w.tick();
            if (!fired.isEmpty()) {
                assertEquals(List.of(7), fired);
                firedAt = i;
                break;
            }
        }
        assertEquals(64L, firedAt);
        assertTrue(w.cascades() >= 1, "expected at least one cascade event");
    }

    @Test
    void cascadeBoundary4096TicksFiresCorrectly() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        w.schedule(4096, 42);
        long firedAt = -1;
        for (long i = 1; i <= 4100; i++) {
            if (!w.tick().isEmpty()) { firedAt = i; break; }
        }
        assertEquals(4096L, firedAt);
        assertTrue(w.cascades() >= 1);
    }

    @Test
    void cancelBeforeFireDropsValue() {
        HierarchicalTimerWheel<String> w = new HierarchicalTimerWheel<>();
        long id = w.schedule(10, "doomed");
        assertTrue(w.cancel(id));
        for (int i = 0; i < 20; i++) assertTrue(w.tick().isEmpty());
    }

    @Test
    void cancelAfterFireReturnsFalse() {
        HierarchicalTimerWheel<String> w = new HierarchicalTimerWheel<>();
        long id = w.schedule(2, "x");
        w.tick();
        List<String> fired = w.tick();
        assertEquals(List.of("x"), fired);
        assertFalse(w.cancel(id), "cancel after fire must return false");
    }

    @Test
    void cancelUnknownIdReturnsFalse() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        assertFalse(w.cancel(99_999L));
    }

    @Test
    void longDelayUsesCoarseWheelThenCascades() {
        long delay = 5000;
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        w.schedule(delay, 1);
        long found = -1;
        for (long i = 1; i <= delay + 5; i++) {
            if (!w.tick().isEmpty()) { found = i; break; }
        }
        assertEquals(delay, found);
    }

    @Test
    void overflowDelayRejectedByTrySchedule() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        long tooBig = HierarchicalTimerWheel.maxDelay();
        TimerError err = assertThrows(TimerError.class, () -> w.trySchedule(tooBig, 1));
        assertEquals(TimerError.Kind.DELAY_TOO_LONG, err.kind());
        assertEquals(tooBig, err.delay());
        assertEquals(tooBig, err.max());
        assertTrue(w.trySchedule(tooBig - 1, 1) >= 0);
    }

    @Test
    void pendingTracksLiveTimersAcrossCascade() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        assertTrue(w.isEmpty());
        w.schedule(5, 1);
        long far = w.schedule(300, 2);
        assertEquals(2, w.pending());
        for (int i = 0; i < 5; i++) w.tick();
        assertEquals(1, w.pending(), "the near timer fired");
        assertTrue(w.cancel(far));
        assertEquals(0, w.pending());
    }

    @Test
    void rescheduleMovesATimerAcrossLevels() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        long id = w.schedule(5000, 9);
        assertTrue(w.reschedule(id, 3), "pull a far timer in to level 0");
        assertEquals(1, w.pending());
        w.tick();
        w.tick();
        assertEquals(List.of(9), w.tick());
        assertFalse(w.reschedule(id, 3), "a fired timer cannot be rescheduled");
        assertFalse(w.reschedule(4242L, 9), "unknown id");
    }

    @Test
    void drainHandsBackEveryPendingTimer() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        for (int d : new int[] {2, 70, 5000}) w.schedule(d, d);
        long cancelled = w.schedule(9, 999);
        assertTrue(w.cancel(cancelled));

        List<Integer> drained = new ArrayList<>(w.drain());
        Collections.sort(drained);
        assertEquals(List.of(2, 70, 5000), drained);
        assertEquals(0, w.pending());
        for (int i = 0; i < 6000; i++) assertTrue(w.tick().isEmpty());
    }

    @Test
    void clearResetsTheTickCounterAndDropsTimers() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        w.schedule(100, 1);
        w.tick();
        w.tick();
        w.clear();
        assertEquals(0L, w.now());
        assertEquals(0, w.pending());
        w.schedule(3, 2);
        w.tick();
        w.tick();
        assertEquals(List.of(2), w.tick());
    }

    @Test
    void manyTimersFireAtCorrectDistinctTicks() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        for (int d = 1; d <= 200; d++) w.schedule(d, d);
        int seen = 0;
        for (int i = 1; i <= 200; i++) {
            List<Integer> fired = w.tick();
            for (Integer v : fired) assertEquals(i, v.intValue(), "delay " + i + " should fire on tick " + i);
            seen += fired.size();
        }
        assertEquals(200, seen);
    }

    @Test
    void cascadesCounterZeroForShortDelays() {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        w.schedule(3, 1);
        w.tick(); w.tick(); w.tick();
        assertEquals(0L, w.cascades());
    }



}
