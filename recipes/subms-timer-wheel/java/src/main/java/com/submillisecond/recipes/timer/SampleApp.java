package com.submillisecond.recipes.timer;

import com.submillisecond.recipes.timer.features.ConcurrentTimerWheel;
import com.submillisecond.recipes.timer.features.CronSchedule;
import com.submillisecond.recipes.timer.features.CronScheduler;
import com.submillisecond.recipes.timer.features.DeadlineScheduler;
import com.submillisecond.recipes.timer.features.HierarchicalTimerWheel;
import com.submillisecond.recipes.timer.features.MeteredTimerWheel;
import com.submillisecond.recipes.timer.features.TestClock;
import com.submillisecond.recipes.timer.features.TimerMetrics;

import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Sample app: an order-lifecycle supervisor on a matching engine, built on
 * {@code subms-timer-wheel}. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.timer.SampleApp}
 *
 * <p>Every resting order arms an expiry timer sized to its time-in-force. A
 * fill cancels it, an amend reschedules it, and end-of-session drains what is
 * left. Session time is a tick counter and the clock-driven sections use an
 * injected clock, so the printed output is identical on every run.
 *
 * <ul>
 *   <li>base               - the TIF supervisor: arm, cancel on fill, amend, drain
 *   <li>hierarchical       - good-til-date orders whose horizons span seconds to a session
 *   <li>concurrent         - quote-timeout timers armed from many market-data threads
 *   <li>deadline-scheduler - FIX session idle timeout, bumped by inbound traffic
 *   <li>cron               - a recurring mark-to-market risk snapshot
 *   <li>metrics            - the supervisor reporting its own cadence
 * </ul>
 */
public final class SampleApp {

    /** What the matching engine hands the supervisor, at a given session second. */
    private record Event(int at, String kind, String order, int tif) { }

    public static void main(String[] args) throws InterruptedException {
        tifSupervisor();
        hierarchicalGtd();
        concurrentQuoteTimeouts();
        deadlineSessionIdle();
        cronRiskSnapshot();
        meteredExpiryWheel();
    }

    /**
     * Base API. One tick is one second of session time. The supervisor holds an
     * order id to timer id map because the wheel hands back a timer id, and the
     * engine only ever speaks order ids.
     */
    static void tifSupervisor() {
        System.out.println("== base: order time-in-force supervisor ==");

        List<Event> tape = List.of(
            new Event(0, "rest", "ORD-A", 3),
            new Event(0, "rest", "ORD-B", 5),
            new Event(0, "rest", "ORD-C", 9),
            new Event(0, "rest", "ORD-D", 12),
            new Event(2, "fill", "ORD-B", 0),
            new Event(4, "amend", "ORD-C", 6));

        TimerWheel<String> expiries = new TimerWheel<>(256);
        Map<String, Long> timerOf = new HashMap<>();

        int sessionSecs = 11;
        for (int second = 0; second <= sessionSecs; second++) {
            for (Event ev : tape) {
                if (ev.at() != second) continue;
                switch (ev.kind()) {
                    case "rest" -> {
                        timerOf.put(ev.order(), expiries.schedule(ev.tif(), ev.order()));
                        System.out.println("  t=" + second + "s rest " + ev.order() + " tif=" + ev.tif() + "s");
                    }
                    case "fill" -> {
                        expiries.cancel(timerOf.get(ev.order()));
                        System.out.println("  t=" + second + "s fill " + ev.order() + " -> expiry cancelled");
                    }
                    default -> {
                        expiries.reschedule(timerOf.get(ev.order()), ev.tif());
                        System.out.println("  t=" + second + "s amend " + ev.order()
                            + " tif -> " + ev.tif() + "s from now");
                    }
                }
            }
            if (second == sessionSecs) break;
            for (String ord : expiries.tick()) {
                System.out.println("  t=" + (second + 1) + "s expire " + ord);
            }
        }

        List<String> unfilled = expiries.drain();
        System.out.println("  session close: " + unfilled.size() + " orders still resting " + unfilled);
        System.out.println("  pending after drain: " + expiries.pending());

        if (!unfilled.equals(List.of("ORD-D"))) {
            throw new AssertionError("only the 12s TIF outlives the session, got " + unfilled);
        }
        if (expiries.pending() != 0) throw new AssertionError("every timer retired");
    }

    /**
     * hierarchical: good-til-date orders expire anywhere from a few seconds to a
     * full session out. The hierarchical wheel holds far-out orders on a coarse
     * level and cascades them down as their deadline approaches, from a fixed
     * 192-bucket footprint.
     */
    static void hierarchicalGtd() {
        System.out.println("\n== hierarchical: good-til-date across horizons ==");
        HierarchicalTimerWheel<String> gtd = new HierarchicalTimerWheel<>();
        gtd.schedule(30, "GTD-near");            // intraday, lands on the fine wheel
        long far = gtd.schedule(5000, "GTD-far"); // deep on the coarse wheel
        System.out.println("  armed 2 GTD orders, " + gtd.pending() + " pending");

        // The desk pulls the far order in to the close of the current session.
        gtd.reschedule(far, 300);
        System.out.println("  GTD-far pulled in to t=300");

        Long nearAt = null, farAt = null;
        for (int t = 1; t <= 300; t++) {
            for (String id : gtd.tick()) {
                if (id.equals("GTD-near")) nearAt = (long) t;
                else if (id.equals("GTD-far")) farAt = (long) t;
            }
        }
        System.out.println("  near fired at t=" + nearAt + ", far fired at t=" + farAt);
        System.out.println("  cascade events: " + gtd.cascades());
        if (nearAt == null || nearAt != 30L) throw new AssertionError("near GTD fires on its deadline");
        if (farAt == null || farAt != 300L) throw new AssertionError("the rescheduled GTD fires on its new deadline");
        if (gtd.cascades() < 1) throw new AssertionError("the far order cascaded down a level");
        if (gtd.pending() != 0) throw new AssertionError("every GTD retired");
    }

    /** concurrent: quote-timeout timers armed from many market-data threads. */
    static void concurrentQuoteTimeouts() throws InterruptedException {
        System.out.println("\n== concurrent: quote timeouts from many feeds ==");
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
        System.out.println("  " + wheel.pending() + " quote timeouts armed across " + feeds + " feeds");

        int fired = wheel.advance(16).size();
        System.out.println("  " + feeds + " feeds x " + perFeed + " quotes -> " + fired + " timeouts fired");
        if (fired != feeds * perFeed) throw new AssertionError("every armed timeout fires once");
        if (!wheel.isEmpty()) throw new AssertionError("the wheel drained itself");
    }

    /**
     * deadline-scheduler: a FIX session must see inbound traffic inside its idle
     * window or be torn down. One timer per session, bumped on every message
     * rather than cancelled and re-armed. A hand-stepped clock keeps the demo
     * deterministic instead of sleeping.
     */
    static void deadlineSessionIdle() {
        System.out.println("\n== deadline-scheduler: FIX session idle timeout ==");
        Duration idle = Duration.ofMillis(30);
        TestClock clock = new TestClock();
        DeadlineScheduler<String> sched = new DeadlineScheduler<>(256, clock, Duration.ofMillis(1));

        long session = sched.scheduleAfter(idle, "SESSION-1");
        long elapsed = 0;
        for (int gap : new int[] {10, 15}) {
            clock.advance(Duration.ofMillis(gap));
            elapsed += gap;
            if (!sched.poll().isEmpty()) throw new AssertionError("traffic keeps the session alive");
            sched.rescheduleAfter(session, idle);
            System.out.println("  inbound msg at +" + elapsed + "ms, idle deadline now +" + (elapsed + 30) + "ms");
        }

        clock.advance(Duration.ofMillis(30));
        List<String> dead = sched.poll();
        System.out.println("  no traffic for " + idle.toMillis() + "ms -> " + dead);
        if (!dead.equals(List.of("SESSION-1"))) throw new AssertionError("the idle timeout fires");
    }

    /** cron: a mark-to-market risk snapshot every 5 minutes, re-arming each fire. */
    static void cronRiskSnapshot() {
        System.out.println("\n== cron: mark-to-market every 5 minutes ==");
        CronSchedule schedule = CronSchedule.parse("*/5 * * * *");
        long start = 1_704_067_201L; // 2024-01-01 00:00:01 UTC
        CronScheduler scheduler = new CronScheduler(schedule, start);

        long first = scheduler.nextFire(start);
        scheduler.recordFire(first);
        long second = scheduler.nextFire(first);
        System.out.println("  first snapshot at epoch " + first + ", next at " + second);
        if (first != 1_704_067_500L) throw new AssertionError("first fire lands on the 5-minute grid");
        if (second != first + 300) throw new AssertionError("re-arms exactly 5 minutes later");
    }

    /**
     * metrics: the supervisor reports its own cadence. A drained timer is
     * counted apart from a fired one, so a session close does not read as a
     * burst of expiries.
     */
    static void meteredExpiryWheel() {
        System.out.println("\n== metrics: self-reporting expiry counters ==");
        MeteredTimerWheel<String> wheel = new MeteredTimerWheel<>(64);
        wheel.schedule(2, "ORD-A");
        long b = wheel.schedule(2, "ORD-B");
        long c = wheel.schedule(2, "ORD-C");
        wheel.cancel(b);
        wheel.reschedule(c, 20);

        int fired = wheel.advance(3).size();
        List<String> left = wheel.drain();
        TimerMetrics m = wheel.metrics();
        System.out.println("  scheduled=" + m.scheduled + " fired=" + m.fired
            + " cancelled=" + m.cancelled + " rescheduled=" + m.rescheduled
            + " drained=" + m.drained + " ticks=" + m.ticks);

        if (m.scheduled != 3 || m.cancelled != 1 || m.rescheduled != 1 || m.fired != 1
            || m.drained != 1 || m.ticks != 3 || fired != 1 || !left.equals(List.of("ORD-C"))) {
            throw new AssertionError("counters must reflect one fire, one cancel, one reschedule");
        }
    }
}
