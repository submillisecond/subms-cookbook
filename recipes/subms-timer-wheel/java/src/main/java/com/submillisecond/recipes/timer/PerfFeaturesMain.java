package com.submillisecond.recipes.timer;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureCategory;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.timer.features.Clock;
import com.submillisecond.recipes.timer.features.ConcurrentTimerWheel;
import com.submillisecond.recipes.timer.features.CronSchedule;
import com.submillisecond.recipes.timer.features.CronScheduler;
import com.submillisecond.recipes.timer.features.DeadlineScheduler;
import com.submillisecond.recipes.timer.features.HierarchicalTimerWheel;
import com.submillisecond.recipes.timer.features.MeteredTimerWheel;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.time.Duration;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.IntConsumer;
import java.util.function.IntFunction;
import java.util.function.ObjIntConsumer;

/**
 * Feature classification bench, the Java mirror of
 * {@code rust/examples/perf_features.rs}. Each feature's representative op is
 * swept across three RESIDENT-TIMER counts,
 * {@link SubMsFeatureManifest#classify} DECIDES the category from the shape of
 * that sweep, and the decision plus a measured p99-by-stage is merge-written
 * into {@code ../.subms/features/java.json}.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it.
 *
 * <p>Resident timers is the sweep axis because it is the only thing that can
 * make a wheel op superlinear: {@code schedule} and the base wheel's
 * {@code cancel} are O(1) by construction, and {@code tick} walks one bucket,
 * whose occupancy is resident/slots. The slot count is held fixed throughout
 * so a slope has one cause.
 *
 * <p>The workload is built so the tick measurement is honest: DUE_PER_TICK
 * timers fire on EVERY measured tick (ticking a wheel with nothing due measures
 * an empty bucket walk and would publish the cheap path); the due rate is FIXED
 * across sweep points (scheduling the resident population uniformly over the
 * horizon would scale the fire rate with N, and the sweep would be reading the
 * workload, not the wheel); and the resident population is scheduled BEYOND the
 * measured window, so it never fires and occupancy holds constant.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.timer.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {

    /**
     * Resident (scheduled, not yet due) timers. A 16x span starting at 32768:
     * the base wheel's bucket holds resident/SLOTS entries, so the smallest
     * point already walks 128 of them and the per-call cost does not dominate
     * the walk.
     */
    private static final int[] SIZES = {32_768, 131_072, 524_288};
    private static final int CANON = SIZES[SIZES.length - 1];
    /**
     * 256 and not the 1024 the standing bench uses. A tick's cost is a fixed
     * per-call part (take the bucket, rebuild the survivors list) plus a
     * per-entry part, and the sweep only measures the second once occupancy is
     * large enough to dominate. At 1024 slots the smallest sweep point walks 32
     * entries, the fixed part is two thirds of it, and the ratio compressed to
     * the point where {@code poll} measured exactly 8.5x over 16x - the
     * classifier's threshold, decided by a rounding. 256 slots quadruples
     * occupancy at the same resident count, which is the cheap way to start the
     * sweep an octave up.
     */
    private static final int SLOTS = 256;
    /** Timers due on each tick. Fixed across sweep points - see the class note. */
    private static final int DUE_PER_TICK = 1;
    /**
     * Untimed ticks before the measured window. Large enough that the tick path
     * is fully C2-compiled before the first sweep point is measured - a point
     * measured while still interpreted reads SLOW, which the sweep sees as a
     * curve that falls with size.
     */
    private static final int WARM_TICKS = 20_000;
    private static final int TIMED_TICKS = 4_096;
    private static final int DUE_TICKS = WARM_TICKS + TIMED_TICKS;
    /**
     * Resident delays sit above the measured window and below the hierarchical
     * wheel's 262144-tick capacity, so one workload builder drives both wheels.
     */
    private static final int HORIZON = 262_000;
    /**
     * Timed reps for a keyed op. Kept well under the smallest sweep point: the
     * timed schedules add to the resident population, and at 10k against 32768
     * that inflation is small enough not to flatten the size axis.
     */
    private static final int OPS = 10_000;
    /**
     * Untimed reps before a keyed measurement, run against a scratch instance
     * so the warm-up does not itself change the resident count being swept.
     */
    private static final int WARM_OPS = 50_000;
    /**
     * Ops per timed sample in a SWEEP. A single {@code schedule} costs tens of
     * nanoseconds and this platform's timer quantum is 100 ns, so an unbatched
     * p50 is pinned to one or two quanta and the category is decided by
     * rounding. 64 and not 16 because the base op at 16 still measures under a
     * microsecond, where one quantum eats the whole margin the classifier's
     * base-delta test works in. Every sweep and the base op use the same batch,
     * so the base-delta test compares like with like; {@code p99ByStage} is
     * measured separately at batch 1 and stays a true per-op figure.
     */
    private static final int BATCH = 64;
    /**
     * Timed reps for a whole-structure op, far too slow to run OPS times. 256
     * and not 32: at 32 reps the reported p99 IS the max, so a single
     * preemption of a sub-millisecond cancel published multiple milliseconds.
     */
    private static final int BULK_REPS = 256;
    /**
     * Bulk warm-up is TIME-BOXED, not a fixed rep count: a fixed count leaves
     * the cheapest sweep point running interpreted while every later point
     * reuses the compiled method, which reads as a curve that falls with size.
     */
    private static final long BULK_WARM_NANOS = 300_000_000L;
    private static final int BULK_WARM_MAX_REPS = 5_000;

    private static final long TICK_NS = 1_000_000L;
    private static final String CRON_EXPR = "*/5 * * * *";
    private static final long EPOCH0 = 1_704_067_200L;

    private record M(long p50, long p99, long max) {}

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // Diagnostic, not a feature: the base wheel's own tick. A single-level
        // wheel decrements the rounds counter of every entry in the bucket it
        // walks, fired or not, so its tick is O(resident/slots) - the cost the
        // hierarchical feature exists to remove. Printed so the feature curves
        // below have something to be read against.
        sweep("base/tick", n -> drain(BATCH, baseWheel(n), TimerWheel::tick));

        hierarchical(manifest);
        concurrent(manifest);
        deadlineScheduler(manifest);
        cron(manifest);
        metrics(manifest);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- hierarchical: cascade across three 64-slot wheels ----------
    private static void hierarchical(SubMsFeatureManifest manifest) {
        // Swept on `tick`, the op the feature transforms. The cascade is the
        // expensive path and it fires on 1 tick in 64 (level 1) and 1 in 4096
        // (level 2), so the measured window has to be long enough to contain
        // both: 4096 timed ticks contains 64 level-1 cascades and one level-2.
        //
        // The curve is flat, and that is the correct reading rather than a
        // hidden cost: a cascade moves the entries in ONE coarse bucket, which
        // holds the timers due in the next 64 (level 1) or 4096 (level 2)
        // ticks. With the due rate held fixed that bucket's size is fixed too,
        // so resident timers further out cost the tick nothing. This is exactly
        // what the level structure buys - the base wheel's own tick, printed
        // above, walks resident/slots entries on EVERY tick.
        long[][] sw = sweep("hierarchical/tick",
                n -> drain(BATCH, hier(n), HierarchicalTimerWheel::tick));

        // `cancel` is the O(resident) op the feature introduces. It has no
        // id->slot index (the base wheel's index would need patching on every
        // cascade) so it sweeps all 192 buckets and every entry in them.
        // Cancelling a MISS walks all of them and is non-destructive, which is
        // what makes it safe to repeat against one input.
        sweep("hierarchical/cancel-miss", n -> bulk(hier(n), w -> w.cancel(Long.MAX_VALUE)));

        // PINNED structural on the strength of `cancel`, not of the swept op.
        // From the source, HierarchicalTimerWheel.cancel iterates LEVELS *
        // SLOTS buckets and every entry in each until it matches, so it is
        // O(resident). The base wheel does not have that op shape: it keeps an
        // id->slot map and cancels in O(bucket). Classifying the feature
        // hot-path off a flat `tick` would tell a reader every op it introduces
        // is safe per-operation, and one of them lands on the millisecond line
        // at half a million timers.
        SubMsFeatureManifest.Decision dec =
                SubMsFeatureManifest.classify(sw, baseP50(), SubMsFeatureCategory.STRUCTURAL);

        HierarchicalTimerWheel<Integer> scratch = new HierarchicalTimerWheel<>();
        HierarchicalTimerWheel<Integer> w = hier(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("tick", drain(1, hier(CANON), HierarchicalTimerWheel::tick).p99());
        p99.put("schedule", keyed(1,
                i -> scratch.schedule(residentDelay(i), 0),
                i -> w.schedule(residentDelay(i), 0)).p99());
        p99.put("cancel", bulk(hier(CANON), x -> x.cancel(Long.MAX_VALUE)).p99());
        manifest.setFeature("hierarchical", dec.category(), p99, dec.reason());
    }

    // ---------- concurrent: short-mutex wrapper ----------
    private static void concurrent(SubMsFeatureManifest manifest) {
        // Swept on `schedule` and measured single-threaded. The feature adds a
        // lock acquire and release to every op; running it contended would
        // measure the contention instead of the indirection, and the thread
        // count would then be a second thing varying across the sweep.
        long[][] sw = sweep("concurrent/schedule", n -> {
            ConcurrentTimerWheel<Integer> scratch = new ConcurrentTimerWheel<>(SLOTS);
            ConcurrentTimerWheel<Integer> w = conc(n);
            return keyed(BATCH,
                    i -> scratch.schedule(residentDelay(i), 0),
                    i -> w.schedule(residentDelay(i), 0));
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50(), null);

        ConcurrentTimerWheel<Integer> scratch = new ConcurrentTimerWheel<>(SLOTS);
        ConcurrentTimerWheel<Integer> w = conc(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("schedule", keyed(1,
                i -> scratch.schedule(residentDelay(i), 0),
                i -> w.schedule(residentDelay(i), 0)).p99());
        p99.put("tick", drain(1, conc(CANON), ConcurrentTimerWheel::tick).p99());
        manifest.setFeature("concurrent", dec.category(), p99, dec.reason());
    }

    // ---------- deadline-scheduler: absolute deadlines over an injected clock ----------
    private static void deadlineScheduler(SubMsFeatureManifest manifest) {
        // Swept on `poll`, the op the layer introduces. With the clock stepped
        // exactly one tick per call, a poll is one wheel tick plus the deadline
        // arithmetic, so the sweep reads the drain the layer is driving.
        long[][] sw = sweep("deadline-scheduler/poll", n -> {
            DeadlineScheduler<Integer> s = sched(n);
            ((StepClock) s.clock()).step = TICK_NS;
            return drain(BATCH, s, DeadlineScheduler::poll);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50(), null);

        DeadlineScheduler<Integer> scratch = sched(0);
        DeadlineScheduler<Integer> s = sched(CANON);
        DeadlineScheduler<Integer> p = sched(CANON);
        ((StepClock) p.clock()).step = TICK_NS;
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("schedule_at", keyed(1,
                i -> scratch.scheduleAt(residentDelay(i) * TICK_NS, 0),
                i -> s.scheduleAt(residentDelay(i) * TICK_NS, 0)).p99());
        p99.put("poll", drain(1, p, DeadlineScheduler::poll).p99());
        manifest.setFeature("deadline-scheduler", dec.category(), p99, dec.reason());
    }

    /**
     * Time only moves when the bench moves it. A free-running clock makes
     * {@code poll} tick however many ticks the host happened to take, which is
     * neither repeatable nor comparable across sweep points; a frozen one makes
     * {@code poll} a no-op and publishes an empty drain as the cost.
     */
    private static final class StepClock implements Clock {
        private long now;
        private long step;

        @Override
        public long nowNanos() {
            now += step;
            return now;
        }
    }

    // ---------- cron: 5-field expression parser + next-fire search ----------
    private static void cron(SubMsFeatureManifest manifest) {
        // Swept on `nextFire`, the op the feature introduces. It searches
        // forward minute by minute from a rolling epoch and never touches a
        // wheel, so it is expected to read FLAT against resident timers - that
        // is the correct result for this feature, not a broken sweep.
        long[][] sw = sweep("cron/next_fire", n -> nextFireRun(BATCH));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50(), null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("parse", keyed(1,
                i -> CronSchedule.parse(CRON_EXPR),
                i -> CronSchedule.parse(CRON_EXPR)).p99());
        p99.put("next_fire", nextFireRun(1).p99());
        manifest.setFeature("cron", dec.category(), p99, dec.reason());
    }

    /** A rolling next-fire walk; each call consumes its own fire so the search never repeats. */
    private static M nextFireRun(int batch) {
        CronScheduler warm = new CronScheduler(CronSchedule.parse(CRON_EXPR), EPOCH0);
        CronScheduler cs = new CronScheduler(CronSchedule.parse(CRON_EXPR), EPOCH0);
        long[] warmEpoch = {EPOCH0};
        long[] epoch = {EPOCH0};
        return keyed(batch,
                i -> {
                    long n = warm.nextFire(warmEpoch[0]);
                    if (n >= 0) {
                        warm.recordFire(n);
                        warmEpoch[0] = n;
                    }
                },
                i -> {
                    long n = cs.nextFire(epoch[0]);
                    if (n >= 0) {
                        cs.recordFire(n);
                        epoch[0] = n;
                    }
                });
    }

    // ---------- metrics: per-instance counters ----------
    private static void metrics(SubMsFeatureManifest manifest) {
        // Swept on `schedule`. The counters are the feature and they sit on the
        // per-op path; sweeping `tick` instead would measure the base wheel's
        // bucket walk and attribute it to a pair of long increments. The tick
        // number is still recorded below so it is visible.
        long[][] sw = sweep("metrics/schedule", n -> {
            MeteredTimerWheel<Integer> scratch = new MeteredTimerWheel<>(SLOTS);
            MeteredTimerWheel<Integer> w = metered(n);
            return keyed(BATCH,
                    i -> scratch.schedule(residentDelay(i), 0),
                    i -> w.schedule(residentDelay(i), 0));
        });
        // PINNED auxiliary. From the source, MeteredTimerWheel.schedule is one
        // increment of an owned long field followed by the base call - no
        // allocation, no branch, no lock. That is well under a nanosecond
        // against a ~55 ns schedule, and nothing on this host resolves half a
        // percent: the base op's own p50 spreads by a quarter across runs, and
        // the feature crossed the classifier's 10% band in both directions on
        // four consecutive runs of unchanged code. Pinning states that a human
        // read the source instead of publishing a coin toss as a measurement.
        SubMsFeatureManifest.Decision dec =
                SubMsFeatureManifest.classify(sw, baseP50(), SubMsFeatureCategory.AUXILIARY);

        MeteredTimerWheel<Integer> scratch = new MeteredTimerWheel<>(SLOTS);
        MeteredTimerWheel<Integer> w = metered(CANON);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("schedule", keyed(1,
                i -> scratch.schedule(residentDelay(i), 0),
                i -> w.schedule(residentDelay(i), 0)).p99());
        p99.put("tick", drain(1, metered(CANON), MeteredTimerWheel::tick).p99());
        manifest.setFeature("metrics", dec.category(), p99, dec.reason());
    }

    // ---------- workload ----------

    /**
     * Delay for the j-th resident timer. Above the measured window so it never
     * fires, spread evenly so bucket occupancy is uniform across the wheel.
     */
    private static int residentDelay(int j) {
        int span = HORIZON - DUE_TICKS - 1;
        return DUE_TICKS + 1 + (j % span);
    }

    /**
     * Loads a wheel with the due stream and {@code n} resident timers via the
     * caller's schedule adapter, which is all the wheel types differ by here.
     */
    private static <W> void load(W w, int n, ObjIntConsumer<W> sched) {
        for (int t = 1; t <= DUE_TICKS; t++) {
            for (int k = 0; k < DUE_PER_TICK; k++) {
                sched.accept(w, t);
            }
        }
        for (int j = 0; j < n; j++) {
            sched.accept(w, residentDelay(j));
        }
    }

    private static TimerWheel<Integer> baseWheel(int n) {
        TimerWheel<Integer> w = new TimerWheel<>(SLOTS);
        load(w, n, (x, d) -> x.schedule(d, 0));
        return w;
    }

    private static HierarchicalTimerWheel<Integer> hier(int n) {
        HierarchicalTimerWheel<Integer> w = new HierarchicalTimerWheel<>();
        load(w, n, (x, d) -> x.schedule(d, 0));
        return w;
    }

    private static ConcurrentTimerWheel<Integer> conc(int n) {
        ConcurrentTimerWheel<Integer> w = new ConcurrentTimerWheel<>(SLOTS);
        load(w, n, (x, d) -> x.schedule(d, 0));
        return w;
    }

    private static MeteredTimerWheel<Integer> metered(int n) {
        MeteredTimerWheel<Integer> w = new MeteredTimerWheel<>(SLOTS);
        load(w, n, (x, d) -> x.schedule(d, 0));
        return w;
    }

    private static DeadlineScheduler<Integer> sched(int n) {
        DeadlineScheduler<Integer> s =
                new DeadlineScheduler<>(SLOTS, new StepClock(), Duration.ofNanos(TICK_NS));
        load(s, n, (x, d) -> x.scheduleAt(d * TICK_NS, 0));
        return s;
    }

    /**
     * The baseline: base {@code schedule}, the O(1) per-op write every feature
     * either decorates or replaces. Re-measured immediately before EACH feature
     * is classified rather than once at the top. Measured once, it sits several
     * half-million-timer builds away from the feature it is compared against,
     * and that gap moves it as much as a real feature delta does.
     */
    private static Long baseP50() {
        TimerWheel<Integer> scratch = new TimerWheel<>(SLOTS);
        TimerWheel<Integer> w = baseWheel(CANON);
        M m = keyed(BATCH,
                i -> scratch.schedule(residentDelay(i), 0),
                i -> w.schedule(residentDelay(i), 0));
        System.err.println("base schedule: p50 " + m.p50() + " p99 " + m.p99() + " max " + m.max());
        return m.p50();
    }

    // ---------- harness plumbing ----------

    /**
     * A per-op measurement. {@code warm} runs against a scratch instance:
     * warming on the measured instance would add WARM_OPS entries to the
     * resident population and compress the size axis at the small end.
     *
     * <p>The warm-up goes THROUGH the harness's timed wrapper, not around it.
     * Warming the op alone leaves the wrapper itself cold, and at batch 64 a
     * measurement only enters it OPS/64 times - 156, far short of what C2
     * needs. That showed up as the first keyed measurements of a run reading
     * 5400 ns and later ones 1600 ns, and as {@code concurrent/schedule}
     * sweeping DOWNWARD across sizes, which is the under-warm signature rather
     * than a feature that gets cheaper with more timers.
     */
    private static M keyed(int batch, IntConsumer warm, IntConsumer op) {
        SubMsPerfHarness wh = new SubMsPerfHarness("timer-feature-warm", "java");
        SubMsPerfHarness.Stage wst = wh.stage("op", WARM_OPS);
        for (int i = 0; i < WARM_OPS; i++) {
            int idx = i;
            wst.time(() -> warm.accept(idx));
        }
        int samples = OPS / batch;
        SubMsPerfHarness h = new SubMsPerfHarness("timer-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", samples);
        for (int s = 0; s < samples; s++) {
            int first = s * batch;
            st.time(() -> {
                for (int k = 0; k < batch; k++) {
                    op.accept(first + k);
                }
            });
        }
        return stat(h);
    }

    /**
     * Ticks a loaded wheel. The warm ticks are untimed and the due stream covers
     * them, so the measured region sees the same fire rate and the same
     * occupancy as the warm region.
     */
    private static <W> M drain(int batch, W w, Consumer<W> tick) {
        for (int i = 0; i < WARM_TICKS; i++) {
            tick.accept(w);
        }
        int samples = TIMED_TICKS / batch;
        SubMsPerfHarness h = new SubMsPerfHarness("timer-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", samples);
        for (int s = 0; s < samples; s++) {
            st.time(() -> {
                for (int k = 0; k < batch; k++) {
                    tick.accept(w);
                }
            });
        }
        return stat(h);
    }

    /**
     * A whole-structure op, repeated against one input built outside the timed
     * region. Only safe for a NON-destructive op - every use here is a cancel of
     * an id that does not exist, which walks the same buckets every rep.
     */
    private static <W> M bulk(W w, Consumer<W> op) {
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(w);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("timer-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", BULK_REPS);
        for (int i = 0; i < BULK_REPS; i++) {
            st.time(() -> op.accept(w));
        }
        return stat(h);
    }

    /**
     * Sweeps and PRINTS the curve, p50 / p99 / max at every point. The
     * classifier reads p50; the other two are here because a ratio-compressed or
     * non-monotonic curve classifies flat and the only way to catch one is to
     * look at the rows.
     */
    private static long[][] sweep(String label, IntFunction<M> at) {
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(':');
        for (int i = 0; i < SIZES.length; i++) {
            M m = at.apply(SIZES[i]);
            rows[i][0] = SIZES[i];
            rows[i][1] = m.p50();
            sb.append(" (").append(SIZES[i]).append(": p50 ").append(m.p50())
                    .append(" p99 ").append(m.p99()).append(" max ").append(m.max()).append(')');
        }
        System.err.println(sb);
        return rows;
    }

    private static M stat(SubMsPerfHarness h) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> new M(s.p50Ns(), s.p99Ns(), s.maxNs()))
                .orElse(new M(0, 0, 0));
    }

    private PerfFeaturesMain() {}
}
