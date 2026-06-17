package com.submillisecond.recipes.timer.features;

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
}
