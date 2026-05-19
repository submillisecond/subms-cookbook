package com.submillisecond.recipes.timer;

import org.junit.jupiter.api.Test;

import java.util.Collections;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TimerWheelTest {

    @Test
    void firesAtCorrectTick() {
        TimerWheel<String> w = new TimerWheel<>(64);
        w.schedule(3, "a");
        assertTrue(w.tick().isEmpty());
        assertTrue(w.tick().isEmpty());
        assertEquals(List.of("a"), w.tick());
    }

    @Test
    void cancelsPendingTimer() {
        TimerWheel<String> w = new TimerWheel<>(64);
        long id = w.schedule(3, "a");
        assertTrue(w.cancel(id));
        for (int i = 0; i < 4; i++) assertTrue(w.tick().isEmpty());
    }

    @Test
    void cancelUnknownIdReturnsFalse() {
        TimerWheel<String> w = new TimerWheel<>(64);
        assertFalse(w.cancel(999));
    }

    @Test
    void multipleTimersSameTickAllFire() {
        TimerWheel<Integer> w = new TimerWheel<>(64);
        w.schedule(2, 1);
        w.schedule(2, 2);
        w.schedule(2, 3);
        assertTrue(w.tick().isEmpty());
        List<Integer> fired = w.tick();
        Collections.sort(fired);
        assertEquals(List.of(1, 2, 3), fired);
    }

    @Test
    void longDelaySpansRevolutions() {
        int n = 16;
        TimerWheel<String> w = new TimerWheel<>(n);
        w.schedule(2 * n + 3, "later");
        for (int i = 0; i < 2 * n + 2; i++) assertTrue(w.tick().isEmpty());
        assertEquals(List.of("later"), w.tick());
    }

    @Test
    void slotsRoundedUpToPowerOfTwo() {
        assertEquals(1024, new TimerWheel<Integer>(1000).numSlots());
        assertEquals(2, new TimerWheel<Integer>(0).numSlots());
    }

    @Test
    void delayZeroFiresOnNextRevolution() {
        TimerWheel<String> w = new TimerWheel<>(64);
        w.schedule(0, "now");
        boolean fired = false;
        for (int i = 0; i < 64; i++) if (!w.tick().isEmpty()) { fired = true; break; }
        assertTrue(fired);
    }

    @Test
    void cancelReturnsFalseAfterFire() {
        TimerWheel<String> w = new TimerWheel<>(64);
        long id = w.schedule(0, "x");
        for (int i = 0; i < 64; i++) if (!w.tick().isEmpty()) break;
        assertFalse(w.cancel(id));
    }

    @Test
    void manyPendingTimersFireAtVariousDelays() {
        // Delays 1..50 each fire on tick(delay); 50 ticks must catch all.
        TimerWheel<Integer> w = new TimerWheel<>(128);
        for (int i = 1; i <= 50; i++) w.schedule(i, i);
        int totalFired = 0;
        for (int i = 0; i < 50; i++) totalFired += w.tick().size();
        assertEquals(50, totalFired);
    }

    @Test
    void ticksWithNoTimersReturnEmpty() {
        TimerWheel<Integer> w = new TimerWheel<>(16);
        for (int i = 0; i < 100; i++) assertTrue(w.tick().isEmpty());
    }
}
