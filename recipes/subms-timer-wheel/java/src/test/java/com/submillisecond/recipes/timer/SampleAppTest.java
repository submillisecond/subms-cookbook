package com.submillisecond.recipes.timer;

import com.submillisecond.recipes.timer.features.ConcurrentTimerWheel;
import com.submillisecond.recipes.timer.features.CronSchedule;
import com.submillisecond.recipes.timer.features.CronScheduler;
import com.submillisecond.recipes.timer.features.DeadlineScheduler;
import com.submillisecond.recipes.timer.features.HierarchicalTimerWheel;
import com.submillisecond.recipes.timer.features.MeteredTimerWheel;
import com.submillisecond.recipes.timer.features.TestClock;
import com.submillisecond.recipes.timer.features.TimerMetrics;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        TimerWheel<String> w = new TimerWheel<>(256);
        w.schedule(5, "hello");                       // fire five ticks out
        for (int i = 0; i < 4; i++) {
            assertTrue(w.tick().isEmpty());           // nothing due yet
        }
        assertEquals(List.of("hello"), w.tick());     // the fifth tick fires it
        // quickstart:end
    }

    @Test
    void tifSupervisorScenario() {
        TimerWheel<String> expiries = new TimerWheel<>(256);
        expiries.schedule(3, "ORD-A");
        long ordB = expiries.schedule(5, "ORD-B");
        long ordC = expiries.schedule(9, "ORD-C");
        expiries.schedule(12, "ORD-D");
        assertEquals(4, expiries.pending());

        List<String> expired = new ArrayList<>();
        expired.addAll(expiries.advance(2));
        expiries.cancel(ordB); // filled at t=2

        expired.addAll(expiries.advance(2));
        expiries.reschedule(ordC, 6); // amended at t=4, now due t=10

        expired.addAll(expiries.advance(7));
        assertEquals(List.of("ORD-A", "ORD-C"), expired, "a cancelled TIF never fires");
        assertEquals(List.of("ORD-D"), expiries.drain(), "the session closes on the long TIF");
        assertEquals(0, expiries.pending(), "every timer retired");
    }

    @Test
    void hierarchicalGtdFiresAndCascades() {
        HierarchicalTimerWheel<String> gtd = new HierarchicalTimerWheel<>();
        gtd.schedule(30, "GTD-near");
        gtd.schedule(300, "GTD-far");

        Long nearAt = null, farAt = null;
        for (int t = 1; t <= 300; t++) {
            for (String id : gtd.tick()) {
                if (id.equals("GTD-near")) nearAt = (long) t;
                else if (id.equals("GTD-far")) farAt = (long) t;
            }
        }
        assertEquals(Long.valueOf(30), nearAt);
        assertEquals(Long.valueOf(300), farAt);
        assertTrue(gtd.cascades() >= 1, "the far order cascaded down a level");
    }

    @Test
    void concurrentQuoteTimeoutsAllFire() throws InterruptedException {
        ConcurrentTimerWheel<Integer> wheel = new ConcurrentTimerWheel<>(256);
        int feeds = 4, perFeed = 50;
        List<Thread> threads = new ArrayList<>();
        for (int feed = 0; feed < feeds; feed++) {
            final int f = feed;
            Thread t = new Thread(() -> {
                for (int i = 0; i < perFeed; i++) wheel.schedule(1 + (i % 8), f * 1000 + i);
            });
            threads.add(t);
            t.start();
        }
        for (Thread t : threads) t.join();

        int fired = 0;
        for (int i = 0; i < 16; i++) fired += wheel.tick().size();
        assertEquals(feeds * perFeed, fired);
    }

    @Test
    void deadlineSessionIdleTimeoutIsBumpedByTraffic() {
        Duration idle = Duration.ofMillis(30);
        TestClock clock = new TestClock();
        DeadlineScheduler<String> sched = new DeadlineScheduler<>(256, clock, Duration.ofMillis(1));
        long session = sched.scheduleAfter(idle, "SESSION-1");

        for (int gap : new int[] {10, 15}) {
            clock.advance(Duration.ofMillis(gap));
            assertTrue(sched.poll().isEmpty(), "traffic keeps the session alive");
            assertTrue(sched.rescheduleAfter(session, idle));
        }

        clock.advance(Duration.ofMillis(30));
        assertEquals(List.of("SESSION-1"), sched.poll());
    }

    @Test
    void cronRiskSnapshotReArms() {
        CronSchedule schedule = CronSchedule.parse("*/5 * * * *");
        long start = 1_704_067_201L;
        CronScheduler scheduler = new CronScheduler(schedule, start);

        long first = scheduler.nextFire(start);
        assertEquals(1_704_067_500L, first);
        scheduler.recordFire(first);
        assertEquals(first + 300, scheduler.nextFire(first));
    }

    @Test
    void meteredExpiryCounters() {
        MeteredTimerWheel<String> wheel = new MeteredTimerWheel<>(64);
        wheel.schedule(2, "ORD-A");
        long b = wheel.schedule(2, "ORD-B");
        long c = wheel.schedule(2, "ORD-C");
        wheel.cancel(b);
        wheel.reschedule(c, 20);

        int fired = wheel.advance(3).size();
        List<String> left = wheel.drain();
        TimerMetrics m = wheel.metrics();
        assertEquals(3, m.scheduled);
        assertEquals(1, m.cancelled);
        assertEquals(1, m.rescheduled);
        assertEquals(1, m.fired);
        assertEquals(List.of("ORD-C"), left);
        assertEquals(1, m.drained);
        assertEquals(3, m.ticks);
        assertEquals(1, fired);
    }

    @Test
    void sampleAppMainRunsEndToEnd() throws InterruptedException {
        SampleApp.main(new String[0]);
    }
}
