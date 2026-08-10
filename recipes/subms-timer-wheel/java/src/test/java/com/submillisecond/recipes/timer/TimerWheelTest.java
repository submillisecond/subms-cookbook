package com.submillisecond.recipes.timer;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
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
    void delayZeroFiresOnTheNextTick() {
        // A deadline already in the past is due immediately; Netty treats it
        // the same way. The finest resolution is one tick.
        TimerWheel<String> w = new TimerWheel<>(64);
        w.schedule(0, "now");
        assertEquals(List.of("now"), w.tick());
    }

    @Test
    void delayThatIsAnExactMultipleOfTheSlotCountFiresOnTime() {
        // The slot for a multiple-of-N delay is the bucket the hand just
        // left, so it is not revisited for a full revolution. Charging a
        // rounds counter for that revolution too would fire a lap late.
        for (int slots : new int[] {16, 64}) {
            for (int laps = 1; laps <= 3; laps++) {
                TimerWheel<Integer> w = new TimerWheel<>(slots);
                int delay = slots * laps;
                w.schedule(delay, delay);
                for (int t = 1; t < delay; t++) {
                    assertTrue(w.tick().isEmpty(), "slots=" + slots + " delay=" + delay + " tick=" + t);
                }
                assertEquals(List.of(delay), w.tick(), "slots=" + slots + " delay=" + delay);
            }
        }
    }

    @Test
    void everyDelayUpToThreeRevolutionsFiresOnItsTick() {
        int slots = 8;
        for (int delay = 1; delay <= 3 * slots; delay++) {
            TimerWheel<Integer> w = new TimerWheel<>(slots);
            w.schedule(delay, delay);
            int firedAt = -1;
            for (int t = 1; t <= 4 * slots; t++) {
                if (!w.tick().isEmpty()) { firedAt = t; break; }
            }
            assertEquals(delay, firedAt, "delay " + delay);
        }
    }

    @Test
    void cancelReturnsFalseAfterFire() {
        TimerWheel<String> w = new TimerWheel<>(64);
        long id = w.schedule(1, "x");
        assertEquals(List.of("x"), w.tick());
        assertFalse(w.cancel(id));
    }

    @Test
    void cancelTwiceReturnsFalseTheSecondTime() {
        TimerWheel<String> w = new TimerWheel<>(64);
        long id = w.schedule(3, "a");
        assertTrue(w.cancel(id));
        assertEquals(0, w.pending(), "cancel retires the id immediately");
        assertFalse(w.cancel(id));
    }

    @Test
    void manyPendingTimersFireAtVariousDelays() {
        // Delays 1..50 each fire on tick(delay); 50 ticks must catch all.
        TimerWheel<Integer> w = new TimerWheel<>(128);
        for (int i = 1; i <= 50; i++) w.schedule(i, i);
        assertEquals(50, w.pending());
        int totalFired = 0;
        for (int i = 0; i < 50; i++) totalFired += w.tick().size();
        assertEquals(50, totalFired);
        assertEquals(0, w.pending());
    }

    @Test
    void ticksWithNoTimersReturnEmpty() {
        TimerWheel<Integer> w = new TimerWheel<>(16);
        for (int i = 0; i < 100; i++) assertTrue(w.tick().isEmpty());
    }

    @Test
    void advanceCatchesUpAndReturnsInTickOrder() {
        TimerWheel<Integer> w = new TimerWheel<>(64);
        w.schedule(1, 1);
        w.schedule(2, 2);
        w.schedule(3, 3);
        assertEquals(List.of(1, 2, 3), w.advance(3));
        assertEquals(List.of(), w.advance(0));
        assertTrue(w.isEmpty());
    }

    @Test
    void rescheduleMovesAPendingTimerAndKeepsItsId() {
        TimerWheel<String> w = new TimerWheel<>(64);
        long id = w.schedule(2, "a");
        assertTrue(w.reschedule(id, 5));
        assertEquals(1, w.pending());
        assertTrue(w.advance(4).isEmpty());
        assertEquals(List.of("a"), w.tick());
        assertFalse(w.cancel(id), "the id retired when the timer fired");
    }

    @Test
    void rescheduleCanPullATimerEarlier() {
        TimerWheel<Integer> w = new TimerWheel<>(64);
        long id = w.schedule(40, 7);
        assertTrue(w.reschedule(id, 1));
        assertEquals(List.of(7), w.tick());
    }

    @Test
    void rescheduleRejectsUnknownCancelledAndFiredIds() {
        TimerWheel<Integer> w = new TimerWheel<>(64);
        assertFalse(w.reschedule(42L, 3), "unknown id");

        long cancelled = w.schedule(3, 1);
        w.cancel(cancelled);
        assertFalse(w.reschedule(cancelled, 3), "cancelled id");

        long fired = w.schedule(1, 2);
        w.tick();
        assertFalse(w.reschedule(fired, 3), "fired id");
    }

    @Test
    void rescheduleSurvivesABucketSharedWithOtherTimers() {
        TimerWheel<Integer> w = new TimerWheel<>(8);
        long a = w.schedule(3, 1);
        w.schedule(3, 2);
        w.schedule(3, 3);
        assertTrue(w.reschedule(a, 6));
        List<Integer> atThree = new ArrayList<>(w.advance(3));
        Collections.sort(atThree);
        assertEquals(List.of(2, 3), atThree);
        assertEquals(List.of(1), w.advance(3));
    }

    @Test
    void drainReturnsEveryPendingTimerAndEmptiesTheWheel() {
        TimerWheel<Integer> w = new TimerWheel<>(16);
        for (int i = 1; i <= 20; i++) w.schedule(i, i);
        long cancelled = w.schedule(4, 999);
        w.cancel(cancelled);

        List<Integer> drained = new ArrayList<>(w.drain());
        Collections.sort(drained);
        assertEquals(20, drained.size());
        assertEquals(1, drained.get(0).intValue());
        assertEquals(20, drained.get(19).intValue());
        assertEquals(0, w.pending());
        assertTrue(w.advance(64).isEmpty(), "nothing left to fire");
    }

    @Test
    void clearDropsPendingTimersAndResetsTheHand() {
        TimerWheel<Integer> w = new TimerWheel<>(16);
        w.schedule(3, 1);
        w.advance(2);
        w.clear();
        assertEquals(0, w.pending());
        w.schedule(3, 2);
        assertEquals(List.of(2), w.advance(3));
    }

    @Test
    void slotLenShowsBucketOccupancy() {
        TimerWheel<Integer> w = new TimerWheel<>(8);
        w.schedule(3, 1);
        w.schedule(11, 2); // 11 & 7 == 3, same bucket, one revolution later
        assertEquals(2, w.slotLen(3));
        assertEquals(0, w.slotLen(4));
        assertEquals(0, w.slotLen(9999), "out of range reads as empty");
        assertEquals(0, w.slotLen(-1));
    }

    @Test
    void tryScheduleRefusesADelayPastCapacity() {
        TimerWheel<Integer> w = new TimerWheel<>(2);
        long max = w.maxDelay();
        assertEquals(2L * Integer.MAX_VALUE, max);
        TimerError err = assertThrows(TimerError.class, () -> w.trySchedule(Long.MAX_VALUE, 1));
        assertEquals(TimerError.Kind.DELAY_TOO_LONG, err.kind());
        assertEquals(Long.MAX_VALUE, err.delay());
        assertEquals(max, err.max());
        assertEquals(0, w.pending(), "a refused schedule arms nothing");
        assertTrue(w.trySchedule(4, 1) > 0);
    }

    @Test
    void scheduleClampsADelayPastCapacityInsteadOfRefusing() {
        TimerWheel<Integer> w = new TimerWheel<>(4);
        long id = w.schedule(Long.MAX_VALUE, 1);
        assertEquals(1, w.pending());
        assertTrue(w.cancel(id));
    }

    @Test
    void negativeDelayFiresOnTheNextTick() {
        TimerWheel<String> w = new TimerWheel<>(16);
        w.schedule(-5, "past-due");
        assertEquals(List.of("past-due"), w.tick());
    }
}
