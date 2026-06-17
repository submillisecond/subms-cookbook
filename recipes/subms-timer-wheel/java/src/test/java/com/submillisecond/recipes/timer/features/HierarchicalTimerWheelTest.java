package com.submillisecond.recipes.timer.features;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
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
        assertEquals(-1L, w.trySchedule(tooBig, 1));
        assertTrue(w.trySchedule(tooBig - 1, 1) >= 0);
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
