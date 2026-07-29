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
import java.util.List;

/**
 * Sample app: a tour of {@code subms-timer-wheel}, base API first, then each
 * optional variant. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.timer.SampleApp}
 *
 * <p>The framing is an order-lifecycle scheduler on a matching engine: every
 * resting order carries a time-in-force, and the wheel is what fires the expiry
 * when the clock reaches it.
 *
 * <ul>
 *   <li>base               - order time-in-force (TIF) expiry on the book
 *   <li>hierarchical       - good-til-date orders whose horizons span seconds to a session
 *   <li>concurrent         - quote-timeout timers armed from many market-data threads
 *   <li>deadline-scheduler - session heartbeat against an absolute wall-clock deadline
 *   <li>cron               - a recurring mark-to-market risk snapshot
 *   <li>metrics            - per-instance scheduled/fired/cancelled/tick counters
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws InterruptedException {
        baseTifExpiry();
        hierarchicalGtd();
        concurrentQuoteTimeouts();
        deadlineHeartbeat();
        cronRiskSnapshot();
        meteredExpiryWheel();
    }

    /** Base API: each resting order arms an expiry timer sized to its TIF. */
    static void baseTifExpiry() {
        System.out.println("== base: order time-in-force expiry ==");
        TimerWheel<String> expiries = new TimerWheel<>(256);

        expiries.schedule(3, "ORD-A");            // 3s TIF
        long ordB = expiries.schedule(5, "ORD-B"); // 5s TIF, but fills early
        expiries.schedule(10, "ORD-C");           // 10s TIF

        expiries.cancel(ordB); // ORD-B fully filled at t=2, cancel its expiry
        System.out.println("  ORD-B filled -> cancelled");

        List<String> expired = new ArrayList<>();
        for (int second = 1; second <= 10; second++) {
            for (String id : expiries.tick()) {
                System.out.println("  t=" + second + "s expire " + id);
                expired.add(id);
            }
        }
        System.out.println("  -> expired " + expired);
        if (!expired.equals(List.of("ORD-A", "ORD-C"))) {
            throw new AssertionError("only uncancelled TIFs fire, got " + expired);
        }
    }

    /** hierarchical: good-til-date orders across horizons; far ones cascade down. */
    static void hierarchicalGtd() {
        System.out.println("\n== hierarchical: good-til-date across horizons ==");
        HierarchicalTimerWheel<String> gtd = new HierarchicalTimerWheel<>();
        gtd.schedule(30, "GTD-near");  // intraday, lands on the fine wheel
        gtd.schedule(300, "GTD-far");  // past 64 ticks, lands on a coarse wheel

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
        if (farAt == null || farAt != 300L) throw new AssertionError("far GTD fires on its deadline");
        if (gtd.cascades() < 1) throw new AssertionError("the far order cascaded down a level");
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

        int fired = 0;
        for (int i = 0; i < 16; i++) fired += wheel.tick().size();
        System.out.println("  " + feeds + " feeds x " + perFeed + " quotes -> " + fired + " timeouts fired");
        if (fired != feeds * perFeed) throw new AssertionError("every armed timeout fires once");
    }

    /** deadline-scheduler: a heartbeat fires at an absolute wall-clock instant. */
    static void deadlineHeartbeat() {
        System.out.println("\n== deadline-scheduler: heartbeat by an absolute instant ==");
        TestClock clock = new TestClock();
        DeadlineScheduler<String> sched = new DeadlineScheduler<>(64, clock, Duration.ofMillis(1));

        long deadlineNanos = Duration.ofMillis(5).toNanos();
        sched.scheduleAt(deadlineNanos, "HEARTBEAT");

        clock.advance(Duration.ofMillis(4));
        List<String> early = sched.poll();
        System.out.println("  at +4ms: " + early);
        if (!early.isEmpty()) throw new AssertionError("nothing fires before the deadline");

        clock.advance(Duration.ofMillis(1));
        List<String> due = sched.poll();
        System.out.println("  at +5ms: " + due);
        if (!due.equals(List.of("HEARTBEAT"))) throw new AssertionError("heartbeat fires at its instant");
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

    /** metrics: the expiry wheel reports its own scheduled/fired/cancelled cadence. */
    static void meteredExpiryWheel() {
        System.out.println("\n== metrics: self-reporting expiry counters ==");
        MeteredTimerWheel<String> wheel = new MeteredTimerWheel<>(64);
        wheel.schedule(2, "ORD-A");
        long b = wheel.schedule(2, "ORD-B");
        wheel.cancel(b);

        int fired = 0;
        for (int i = 0; i < 3; i++) fired += wheel.tick().size();
        TimerMetrics m = wheel.metrics();
        System.out.println("  scheduled=" + m.scheduled + " fired=" + m.fired
            + " cancelled=" + m.cancelled + " ticks=" + m.ticks);
        if (m.scheduled != 2 || m.cancelled != 1 || m.fired != 1 || m.ticks != 3 || fired != 1) {
            throw new AssertionError("counters must reflect one fire, one cancel");
        }
    }
}
