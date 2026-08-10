package com.submillisecond.recipes.timer.features;

import java.util.ArrayList;
import java.util.Collections;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class DeadlineSchedulerTest {

    private DeadlineScheduler<String> newScheduler(TestClock c) {
        return new DeadlineScheduler<>(64, c, Duration.ofMillis(1));
    }

    @Test
    void scheduleAfterFiresAfterElapsedTime() {
        TestClock c = new TestClock();
        DeadlineScheduler<String> s = newScheduler(c);
        s.scheduleAfter(Duration.ofMillis(3), "a");
        assertTrue(s.poll().isEmpty());
        c.advance(Duration.ofMillis(2));
        assertTrue(s.poll().isEmpty());
        c.advance(Duration.ofMillis(1));
        assertEquals(List.of("a"), s.poll());
    }

    @Test
    void scheduleAtWithAbsoluteDeadlineFiresWhenClockPassesIt() {
        TestClock c = new TestClock();
        DeadlineScheduler<String> s = newScheduler(c);
        long when = c.nowNanos() + Duration.ofMillis(5).toNanos();
        s.scheduleAt(when, "five");
        c.advance(Duration.ofMillis(4));
        assertTrue(s.poll().isEmpty());
        c.advance(Duration.ofMillis(1));
        assertEquals(List.of("five"), s.poll());
    }

    @Test
    void scheduleAtInThePastFiresOnNextTick() {
        TestClock c = new TestClock();
        DeadlineScheduler<String> s = newScheduler(c);
        c.advance(Duration.ofSeconds(10));
        long id = s.scheduleAt(0L, "stale");
        c.advance(Duration.ofMillis(1));
        assertEquals(List.of("stale"), s.poll());
        assertFalse(s.cancel(id));
    }

    @Test
    void cancelRemovesBeforeFire() {
        TestClock c = new TestClock();
        DeadlineScheduler<String> s = newScheduler(c);
        long id = s.scheduleAfter(Duration.ofMillis(3), "doomed");
        assertTrue(s.cancel(id));
        c.advance(Duration.ofMillis(10));
        assertTrue(s.poll().isEmpty());
    }

    @Test
    void pollWithNoClockMovementIsIdempotent() {
        TestClock c = new TestClock();
        DeadlineScheduler<String> s = newScheduler(c);
        s.scheduleAfter(Duration.ofMillis(2), "a");
        assertTrue(s.poll().isEmpty());
        assertTrue(s.poll().isEmpty());
        c.advance(Duration.ofMillis(2));
        List<String> first = s.poll();
        List<String> second = s.poll();
        assertEquals(List.of("a"), first);
        assertTrue(second.isEmpty(), "second poll must not refire");
    }

    @Test
    void subTickDelayRoundsUpToOneTick() {
        TestClock c = new TestClock();
        DeadlineScheduler<String> s = newScheduler(c);
        s.scheduleAfter(Duration.ofNanos(500_000), "a");
        c.advance(Duration.ofMillis(1));
        assertEquals(List.of("a"), s.poll());
    }

    @Test
    void manyDeadlinesFireInOrder() {
        TestClock c = new TestClock();
        DeadlineScheduler<Integer> s = new DeadlineScheduler<>(64, c, Duration.ofMillis(1));
        for (int i = 1; i <= 10; i++) s.scheduleAfter(Duration.ofMillis(i), i);
        for (int i = 1; i <= 10; i++) {
            c.advance(Duration.ofMillis(1));
            assertEquals(List.of(i), s.poll());
        }
    }

    @Test
    void monotonicClockDoesNotPanic() {
        MonotonicClock m = new MonotonicClock();
        long a = m.nowNanos();
        long b = m.nowNanos();
        assertTrue(b >= a);
    }

    @Test
    void idleTimeoutIsBumpedByRescheduleRatherThanReArmed() {
        TestClock clock = new TestClock();
        DeadlineScheduler<String> s = new DeadlineScheduler<>(256, clock, Duration.ofMillis(1));

        long id = s.scheduleAfter(Duration.ofMillis(10), "SESSION-IDLE");
        assertEquals(1, s.pending());
        assertFalse(s.isEmpty());

        clock.advance(Duration.ofMillis(6));
        assertTrue(s.poll().isEmpty());
        assertTrue(s.rescheduleAfter(id, Duration.ofMillis(10)));

        clock.advance(Duration.ofMillis(9));
        assertTrue(s.poll().isEmpty(), "the bumped deadline has not arrived");
        clock.advance(Duration.ofMillis(1));
        assertEquals(List.of("SESSION-IDLE"), s.poll());
        assertTrue(s.isEmpty());
    }

    @Test
    void rescheduleAtMovesAnAbsoluteDeadlineAndDrainEmptiesTheLayer() {
        TestClock clock = new TestClock();
        DeadlineScheduler<String> s = new DeadlineScheduler<>(256, clock, Duration.ofMillis(1));

        long id = s.scheduleAt(Duration.ofMillis(20).toNanos(), "A");
        assertTrue(s.rescheduleAt(id, Duration.ofMillis(3).toNanos()));
        clock.advance(Duration.ofMillis(3));
        assertEquals(List.of("A"), s.poll());

        assertFalse(s.rescheduleAt(id, 0), "a fired deadline cannot be moved");

        s.scheduleAfter(Duration.ofMillis(5), "B");
        s.scheduleAfter(Duration.ofMillis(9), "C");
        List<String> left = new ArrayList<>(s.drain());
        Collections.sort(left);
        assertEquals(List.of("B", "C"), left);
        assertEquals(0, s.pending());
    }
}
