package com.submillisecond.recipes.timer;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.timer.features.ConcurrentTimerWheel;
import com.submillisecond.recipes.timer.features.CronSchedule;
import com.submillisecond.recipes.timer.features.CronScheduler;
import com.submillisecond.recipes.timer.features.DeadlineScheduler;
import com.submillisecond.recipes.timer.features.HierarchicalTimerWheel;
import com.submillisecond.recipes.timer.features.MeteredTimerWheel;
import com.submillisecond.recipes.timer.features.MonotonicClock;

import java.io.IOException;
import java.time.Duration;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per feature variant - base_schedule, base_tick,
 * hierarchical_schedule, hierarchical_tick, concurrent_schedule,
 * concurrent_tick, deadline_scheduler_schedule_at, deadline_scheduler_poll,
 * cron_schedule, cron_next_match, metrics_schedule, metrics_tick - with the
 * SAME stage names as the Rust bench so the cookbook FeaturePicker columns
 * line up across languages. JSON contract goes to stdout.
 *
 * <p>Each variant times its schedule path and its tick/poll drain path
 * against a deterministic LCG-driven delay stream (seed 0). The deadline +
 * cron layers run on the real {@code MonotonicClock} / epoch clocks they
 * ship rather than a {@code TestClock} so the recorded numbers reflect a
 * real driving workload.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.timer.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final long SEED = 0L;
    private static final int SLOTS = 1024;
    private static final int WARMUP = 20_000;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("timer-wheel-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Long.toString(SEED));
        h.input("slots", Integer.toString(SLOTS));
        h.meta("subms.recipe.slug", "subms-timer-wheel");
        h.meta("subms.recipe.category", "scheduling");

        // ---------- base ----------
        {
            h.meta("subms.workload.feature", "base");
            // Warm schedule + tick to C2 on a throwaway wheel. Scheduling on the
            // real wheel would change what the measured tick pass drains, so the
            // warm wheel is discarded before the timed loops run.
            {
                TimerWheel<Integer> warm = new TimerWheel<>(SLOTS);
                Lcg wr = new Lcg(SEED);
                for (int i = 0; i < WARMUP; i++) warm.schedule((int) wr.bounded(SLOTS * 4L), i);
                for (int i = 0; i < SLOTS * 5; i++) warm.tick();
            }

            TimerWheel<Integer> w = new TimerWheel<>(SLOTS);
            Lcg rng = new Lcg(SEED);
            SubMsPerfHarness.Stage s = h.stage("base_schedule", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                int delay = (int) rng.bounded(SLOTS * 4L);
                long t0 = SubMsTimer.nanosNow();
                w.schedule(delay, i);
                s.record(SubMsTimer.nanosNow() - t0);
            }
            int ticks = SLOTS * 5;
            SubMsPerfHarness.Stage st = h.stage("base_tick", ticks).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ticks; i++) {
                long t0 = SubMsTimer.nanosNow();
                w.tick();
                st.record(SubMsTimer.nanosNow() - t0);
            }
        }

        // ---------- hierarchical ----------
        {
            h.meta("subms.workload.feature", "hierarchical");
            long cap = HierarchicalTimerWheel.maxDelay();
            {
                HierarchicalTimerWheel<Integer> warm = new HierarchicalTimerWheel<>();
                Lcg wr = new Lcg(SEED);
                for (int i = 0; i < WARMUP; i++) warm.schedule(wr.bounded(cap), i);
                for (int i = 0; i < SLOTS * 5; i++) warm.tick();
            }

            HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
            Lcg rng = new Lcg(SEED);
            SubMsPerfHarness.Stage s = h.stage("hierarchical_schedule", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                long delay = rng.bounded(cap);
                long t0 = SubMsTimer.nanosNow();
                w.schedule(delay, i);
                s.record(SubMsTimer.nanosNow() - t0);
            }
            int ticks = SLOTS * 5;
            SubMsPerfHarness.Stage st = h.stage("hierarchical_tick", ticks).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ticks; i++) {
                long t0 = SubMsTimer.nanosNow();
                w.tick();
                st.record(SubMsTimer.nanosNow() - t0);
            }
        }

        // ---------- concurrent (single-threaded path) ----------
        {
            h.meta("subms.workload.feature", "concurrent");
            {
                ConcurrentTimerWheel<Integer> warm = new ConcurrentTimerWheel<>(SLOTS);
                Lcg wr = new Lcg(SEED);
                for (int i = 0; i < WARMUP; i++) warm.schedule((int) wr.bounded(SLOTS * 4L), i);
                for (int i = 0; i < SLOTS * 5; i++) warm.tick();
            }

            ConcurrentTimerWheel<Integer> w = new ConcurrentTimerWheel<>(SLOTS);
            Lcg rng = new Lcg(SEED);
            SubMsPerfHarness.Stage s = h.stage("concurrent_schedule", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                int delay = (int) rng.bounded(SLOTS * 4L);
                long t0 = SubMsTimer.nanosNow();
                w.schedule(delay, i);
                s.record(SubMsTimer.nanosNow() - t0);
            }
            int ticks = SLOTS * 5;
            SubMsPerfHarness.Stage st = h.stage("concurrent_tick", ticks).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ticks; i++) {
                long t0 = SubMsTimer.nanosNow();
                w.tick();
                st.record(SubMsTimer.nanosNow() - t0);
            }
        }

        // ---------- deadline-scheduler ----------
        {
            h.meta("subms.workload.feature", "deadline-scheduler");
            {
                DeadlineScheduler<Integer> warm = new DeadlineScheduler<>(
                        SLOTS, new MonotonicClock(), Duration.ofMillis(1));
                Lcg wr = new Lcg(SEED);
                long warmBase = new MonotonicClock().nowNanos();
                for (int i = 0; i < WARMUP; i++) {
                    warm.scheduleAt(warmBase + wr.bounded(SLOTS * 4L) * 1_000_000L, 0);
                }
                for (int i = 0; i < SLOTS * 5; i++) warm.poll();
            }

            DeadlineScheduler<Integer> sched =
                    new DeadlineScheduler<>(SLOTS, new MonotonicClock(), Duration.ofMillis(1));
            Lcg rng = new Lcg(SEED);
            // Absolute deadlines spread across a few-second horizon off a
            // monotonic origin; scheduleAt maps them onto the wheel.
            long base = new MonotonicClock().nowNanos();
            SubMsPerfHarness.Stage s = h.stage("deadline_scheduler_schedule_at", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                long offsetMs = rng.bounded(SLOTS * 4L);
                long when = base + offsetMs * 1_000_000L;
                long t0 = SubMsTimer.nanosNow();
                sched.scheduleAt(when, 0);
                s.record(SubMsTimer.nanosNow() - t0);
            }
            int polls = SLOTS * 5;
            SubMsPerfHarness.Stage sp = h.stage("deadline_scheduler_poll", polls).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < polls; i++) {
                long t0 = SubMsTimer.nanosNow();
                sched.poll();
                sp.record(SubMsTimer.nanosNow() - t0);
            }
        }

        // ---------- cron ----------
        {
            h.meta("subms.workload.feature", "cron");
            // Parse the expression once, outside the timed loop - parsing is a
            // one-shot setup cost, not a per-fire cost.
            CronSchedule schedule = CronSchedule.parse("*/5 * * * *");

            // Warm the schedule path on a throwaway wheel.
            {
                TimerWheel<Integer> warm = new TimerWheel<>(SLOTS);
                Lcg wr = new Lcg(SEED);
                for (int i = 0; i < WARMUP; i++) warm.schedule((int) wr.bounded(SLOTS * 4L), i);
            }

            // Re-arm path: schedule a recurring fire onto the base wheel each
            // iteration, mirroring how a CronScheduler drives a wheel.
            TimerWheel<Integer> w = new TimerWheel<>(SLOTS);
            Lcg rng = new Lcg(SEED);
            SubMsPerfHarness.Stage s = h.stage("cron_schedule", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                int delay = (int) rng.bounded(SLOTS * 4L);
                long t0 = SubMsTimer.nanosNow();
                w.schedule(delay, i);
                s.record(SubMsTimer.nanosNow() - t0);
            }

            // Warm the next-match computation on a throwaway scheduler walking
            // its own rolling epoch; the measured scheduler then starts fresh.
            {
                CronScheduler warm = new CronScheduler(schedule, 1_704_067_200L);
                long e = 1_704_067_200L;
                for (int i = 0; i < WARMUP; i++) {
                    long next = warm.nextFire(e);
                    if (next >= 0) {
                        warm.recordFire(next);
                        e = next;
                    }
                }
            }

            // next-match path: compute the next firing second from a rolling
            // epoch, the hot loop a CronScheduler runs on every re-arm.
            CronScheduler cs = new CronScheduler(schedule, 1_704_067_200L);
            long epoch = 1_704_067_200L;
            SubMsPerfHarness.Stage sn = h.stage("cron_next_match", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                long t0 = SubMsTimer.nanosNow();
                long next = cs.nextFire(epoch);
                sn.record(SubMsTimer.nanosNow() - t0);
                if (next >= 0) {
                    cs.recordFire(next);
                    epoch = next;
                }
            }
        }

        // ---------- metrics ----------
        {
            h.meta("subms.workload.feature", "metrics");
            {
                MeteredTimerWheel<Integer> warm = new MeteredTimerWheel<>(SLOTS);
                Lcg wr = new Lcg(SEED);
                for (int i = 0; i < WARMUP; i++) warm.schedule((int) wr.bounded(SLOTS * 4L), i);
                for (int i = 0; i < SLOTS * 5; i++) warm.tick();
            }

            MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(SLOTS);
            Lcg rng = new Lcg(SEED);
            SubMsPerfHarness.Stage s = h.stage("metrics_schedule", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                int delay = (int) rng.bounded(SLOTS * 4L);
                long t0 = SubMsTimer.nanosNow();
                w.schedule(delay, i);
                s.record(SubMsTimer.nanosNow() - t0);
            }
            int ticks = SLOTS * 5;
            SubMsPerfHarness.Stage st = h.stage("metrics_tick", ticks).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ticks; i++) {
                long t0 = SubMsTimer.nanosNow();
                w.tick();
                st.record(SubMsTimer.nanosNow() - t0);
            }
        }

        h.writeJson(System.out);
    }

    /** Deterministic LCG mirroring the harness {@code SubMsLcg} so the
     *  delay stream matches the Rust bench's driving workload shape. */
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed | 1L;
        }

        long nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (state >>> 32) & 0xFFFFFFFFL;
        }

        long bounded(long n) {
            if (n <= 0L) return 0L;
            return nextU32() % n;
        }
    }
}
