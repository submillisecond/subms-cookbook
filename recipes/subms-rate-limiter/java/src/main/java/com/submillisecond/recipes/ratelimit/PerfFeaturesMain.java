package com.submillisecond.recipes.ratelimit;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ratelimit.features.Clock;
import com.submillisecond.recipes.ratelimit.features.DistributedLimiter;
import com.submillisecond.recipes.ratelimit.features.HierarchicalLimiter;
import com.submillisecond.recipes.ratelimit.features.InMemoryBackend;
import com.submillisecond.recipes.ratelimit.features.MeteredTokenBucket;
import com.submillisecond.recipes.ratelimit.features.TokenBucket;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.IntFunction;
import java.util.function.Supplier;

/**
 * Feature classification bench, the Java mirror of
 * {@code rust/examples/perf_features.rs}. Each feature's representative op is
 * swept across three FLEET SIZES, {@link SubMsFeatureManifest#classify} DECIDES
 * the category from the shape of that sweep, and the decision plus a measured
 * p99-by-stage is merge-written into {@code ../.subms/features/java.json}.
 *
 * <p>The sweep axis is the number of independent limited entities the workload
 * cycles over: buckets for {@code token-bucket} and {@code metrics}, children
 * for {@code hierarchical}, live keys for {@code distributed-backend}. A
 * limiter has no internal array to grow, so the only thing a deployment scales
 * is the fleet of tenants, and that is the axis every feature here shares.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category
 * is an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.ratelimit.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {

    /**
     * 1024 / 8192 / 65536 limiters, a 64x span.
     *
     * <p>The middle point reads about 1.8x the other two for
     * {@code token-bucket} on this JVM, reproducibly and in either sweep
     * direction (checked by running the sizes descending: 8192 stayed the slow
     * one, so it is the size and not the position). A fleet of 8192 buckets
     * plus their per-op BigInteger garbage is the size that neither fits in
     * cache nor streams, and it is small enough to keep being copied by young
     * collections instead of promoted once. It does not reach the classifier,
     * which reads the smallest and largest points, and both ports agree those
     * two are flat.
     */
    private static final int[] SIZES = {1024, 8192, 65536};
    private static final int CANON_N = SIZES[SIZES.length - 1];
    /**
     * Timed ops per measurement, fixed across the sweep so a slope has one
     * cause. Equal to the largest fleet, so every limiter is touched and the
     * FILL scales with the size instead of a fixed slice of the fleet carrying
     * the whole workload.
     */
    private static final int OPS = CANON_N;

    /**
     * The distributed backend's per-call cost is the counter-map GC, so its
     * span is in live keys and it has to start high enough that the fixed
     * per-call work (a key object, a hash, a lock) does not compress the ratio.
     */
    private static final int[] DIST_SIZES = {512, 2048, 8192};
    private static final int DIST_CANON = DIST_SIZES[DIST_SIZES.length - 1];
    private static final int DIST_OPS = 4_096;
    /**
     * Timed calls hit a FIXED-size hot subset of the prefilled keys, so each
     * hot key takes the same number of bumps at every sweep point and the
     * accept ratio does not move with N. The cold keys still sit in the map and
     * are still swept by the GC - which is the cost being measured.
     */
    private static final int DIST_HOT = 256;
    /**
     * DIST_OPS / DIST_HOT = 16 bumps per hot key on top of the prefill's one,
     * so counts run 2..17 and a limit of 9 splits them exactly in half.
     */
    private static final long DIST_LIMIT = 9L;
    /**
     * An hour. The window must not roll during a run: a roll expires every
     * prefilled key at once and the map collapses to the hot subset, which
     * would silently delete the size axis.
     */
    private static final long DIST_WINDOW_NS = 3_600_000_000_000L;

    /** Synthetic ns added per clock read. See {@link SteppingClock}. */
    private static final long STRIDE_NS = 1_000L;

    /**
     * Bucket capacity, and the rates that put the accept ratio near half.
     * {@code TokenBucket.tryAcquire} reads the clock once, so it accrues
     * {@code TB_RATE * STRIDE_NS / 1e9} = 0.5 tokens per call.
     * {@code MeteredTokenBucket} reads it three times (available, tryAcquire,
     * available), so its rate is a third of that to land on the same ratio.
     */
    private static final long TB_CAP = 4L;
    private static final double TB_RATE = 500_000.0;
    private static final double MET_RATE = 166_667.0;
    /**
     * Untimed acquires each bucket takes during setup, enough to spend the
     * capacity it is built full with and settle into the alternating
     * accept/reject steady state. Without it the accept ratio is a function of
     * how many timed calls each bucket receives, which is {@code OPS / n} - so
     * the mix, not the size, is what the sweep varies: measured 55% grants at
     * 1024 buckets, 88% at 8192 and 100% at 65536, where each bucket is touched
     * once and a full bucket cannot do anything but grant.
     *
     * <p>Odd-indexed buckets take one extra, which is what makes the ratio hold
     * at the top of the sweep. A settled bucket alternates, so a fleet settled
     * in LOCKSTEP still grants on every first touch - 100%, not 50%, at the
     * size where each bucket is touched exactly once. Half the fleet has to be
     * settled on the other phase for the mix to come from across the fleet.
     */
    private static final int PRE_DRAIN = 16;
    /**
     * The parent is deliberately not the throttle: it accrues 2 tokens per call
     * against the 1 it can spend, so it always grants and the child governs the
     * accept ratio. A parent that also rejected would change the mix of paths
     * taken as the fleet grew.
     */
    private static final long HIER_PARENT_CAP = 64L;
    private static final double HIER_PARENT_RATE = 2_000_000.0;

    private static final double BASE_RATE = 1_000_000.0;
    private static final long BASE_BURST = 4L;

    /**
     * Warm is TIME-BOXED rather than a fixed rep count. A fixed count leaves
     * the first sweep point running interpreted while every later point reuses
     * the compiled method, which reads as a cost that FALLS with size.
     */
    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_MAX_REPS = 200_000;

    /**
     * Ops per timed sample on the classification pass. The platform timer ticks
     * at 100 ns on this box and a bucket acquire is a few tens of ns, so a
     * per-op sample can only read 0 or 100 - base and {@code token-bucket} both
     * landed on exactly 100 and the classifier called a lock-guarded refill a
     * non-effect. A batch of 16 puts the sample an order of magnitude above the
     * tick. The published p99-by-stage is still measured one op at a time: a
     * batch mean would hide the tail, which is the number the site prints.
     */
    private static final int BATCH = 16;

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: a plain tryAcquire on the base GCRA limiter, cycled
        // over a fleet the same size as the canonical sweep point so it pays
        // the same cache footprint the features do. Every call is granted;
        // GCRA's reject path returns before the CAS, so the grant is the dearer
        // branch and the conservative baseline.
        Measured base = measure(PerfFeaturesMain::baseFleet,
                (v, i) -> v[i % v.length].tryAcquire(), OPS, BATCH);
        long baseP50 = base.p50;
        System.err.println("base try_acquire over " + CANON_N + " limiters: p50=" + baseP50
                + "ns p99=" + base.p99 + "ns accept=" + pct(base.accept));
        // One hammered limiter, for context only. It is not the classifier's
        // base: comparing a fleet-cycling feature against a single hot limiter
        // would charge the feature for the cache misses the workload shape
        // causes.
        RateLimiter[] one = {new RateLimiter(BASE_RATE, 2L * OPS)};
        Measured hot = measure(() -> one, (v, i) -> v[0].tryAcquire(), OPS, BATCH);
        System.err.println("base try_acquire on 1 hot limiter: p50=" + hot.p50 + "ns p99="
                + hot.p99 + "ns (context only)");

        tokenBucket(manifest, baseP50);
        hierarchical(manifest, baseP50);
        distributedBackend(manifest, baseP50);
        metrics(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    private static RateLimiter[] baseFleet() {
        RateLimiter[] v = new RateLimiter[CANON_N];
        for (int i = 0; i < v.length; i++) {
            v[i] = new RateLimiter(BASE_RATE, BASE_BURST);
        }
        return v;
    }

    // ---------- token-bucket: lock-guarded refill + batch drain ----------
    private static void tokenBucket(SubMsFeatureManifest manifest, long baseP50) {
        long[][] rows = new long[SIZES.length][2];
        Measured canon = sweep("token-bucket/try_acquire", SIZES, rows,
                n -> measure(() -> tbFleet(n), (v, i) -> v[i % v.length].tryAcquire(1L),
                        OPS, BATCH));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(rows, baseP50, null);

        Measured avail = measure(() -> tbFleet(CANON_N), (v, i) -> {
            v[i % v.length].available();
            return true;
        }, OPS, BATCH);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("try_acquire", canon.p99);
        p99.put("available", avail.p99);
        manifest.setFeature("token-bucket", dec.category(), p99, dec.reason());
    }

    private static TokenBucket[] tbFleet(int n) {
        TokenBucket[] v = new TokenBucket[n];
        for (int i = 0; i < n; i++) {
            v[i] = new TokenBucket(TB_CAP, TB_RATE, new SteppingClock());
            for (int d = 0; d < PRE_DRAIN + (i & 1); d++) {
                v[i].tryAcquire(1L);
            }
        }
        return v;
    }

    // ---------- hierarchical: child AND parent must both grant ----------
    private static void hierarchical(SubMsFeatureManifest manifest, long baseP50) {
        // Swept on the CHILD COUNT, which is the only thing this feature can
        // scale. It is not a parent CHAIN - the source holds one parent and a
        // flat list of children, and a call is a list index plus a fixed three
        // bucket operations - so the cost is expected to be flat and the sweep
        // is what says so rather than a reading of the code.
        long[][] rows = new long[SIZES.length][2];
        Measured canon = sweep("hierarchical/try_acquire", SIZES, rows,
                n -> measure(() -> hier(n),
                        (h, i) -> h.tryAcquire(i % h.numChildren(), 1L), OPS, BATCH));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(rows, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("try_acquire", canon.p99);
        manifest.setFeature("hierarchical", dec.category(), p99, dec.reason());
    }

    private static HierarchicalLimiter hier(int n) {
        HierarchicalLimiter h = new HierarchicalLimiter(
                HIER_PARENT_CAP, HIER_PARENT_RATE, n, TB_CAP, TB_RATE, SteppingClock::new);
        for (int c = 0; c < h.numChildren(); c++) {
            for (int d = 0; d < PRE_DRAIN + (c & 1); d++) {
                h.tryAcquire(c, 1L);
            }
        }
        return h;
    }

    // ---------- distributed-backend: fixed-window counters ----------
    private static void distributedBackend(SubMsFeatureManifest manifest, long baseP50) {
        // Keys are built once, outside every timed region: formatting one
        // inside the loop would put string construction in the measurement.
        // Fixed width so key length is not a second variable.
        String[] keys = new String[DIST_CANON];
        for (int i = 0; i < keys.length; i++) {
            keys[i] = String.format("key-%06d", i);
        }

        long[][] rows = new long[DIST_SIZES.length][2];
        Measured canon = sweep("distributed-backend/try_acquire", DIST_SIZES, rows,
                n -> measure(() -> prefilled(keys, n),
                        (d, i) -> d.tryAcquire(keys[i % DIST_HOT]), DIST_OPS, 1));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(rows, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("try_acquire", canon.p99);
        manifest.setFeature("distributed-backend", dec.category(), p99, dec.reason());
    }

    private static DistributedLimiter prefilled(String[] keys, int n) {
        DistributedLimiter d = new DistributedLimiter(
                new InMemoryBackend(), DIST_LIMIT, DIST_WINDOW_NS, new SteppingClock());
        for (int i = 0; i < n; i++) {
            d.tryAcquire(keys[i]);
        }
        return d;
    }

    // ---------- metrics: counters around the same bucket ----------
    private static void metrics(SubMsFeatureManifest manifest, long baseP50) {
        long[][] rows = new long[SIZES.length][2];
        Measured canon = sweep("metrics/try_acquire", SIZES, rows,
                n -> measure(() -> metFleet(n), (v, i) -> v[i % v.length].tryAcquire(1L),
                        OPS, BATCH));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(rows, baseP50, null);

        Measured snap = measure(() -> metFleet(CANON_N), (v, i) -> {
            v[i % v.length].snapshot();
            return true;
        }, OPS, BATCH);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("try_acquire", canon.p99);
        p99.put("snapshot", snap.p99);
        manifest.setFeature("metrics", dec.category(), p99, dec.reason());
    }

    private static MeteredTokenBucket[] metFleet(int n) {
        MeteredTokenBucket[] v = new MeteredTokenBucket[n];
        for (int i = 0; i < n; i++) {
            v[i] = new MeteredTokenBucket(TB_CAP, MET_RATE, new SteppingClock());
            for (int d = 0; d < PRE_DRAIN + (i & 1); d++) {
                v[i].tryAcquire(1L);
            }
        }
        return v;
    }

    // ---------- harness plumbing ----------

    /**
     * A clock that reads the platform clock and then throws the reading away,
     * returning a synthetic value that steps by a fixed {@code STRIDE_NS} per
     * read.
     *
     * <p>Both halves are load-bearing. The read is kept because the production
     * {@code SystemClock} makes exactly that call, and a fixture that skipped
     * it would hand every feature a free saving the base limiter still pays,
     * which reads as "cheaper than base" - auxiliary - for a feature that is
     * not. The value is synthetic because real elapsed time between two touches
     * of the SAME bucket scales with how many buckets the loop cycles: at 65536
     * buckets a bucket sees milliseconds between its own calls and refills to
     * full every time, so the sweep would be varying token occupancy rather
     * than size.
     *
     * <p>A frozen clock is the other half of the same trap: {@code refillLocked}
     * early-returns on {@code elapsed <= 0}, so a bucket driven by a stopped
     * clock never runs the refill arithmetic - here a BigInteger multiply,
     * divide, add and min, the dominant cost of the call - and the feature
     * measures as a compare-and-subtract.
     */
    private static final class SteppingClock implements Clock {
        private final long origin = SubMsTimer.nanosNow();
        private final AtomicLong steps = new AtomicLong();
        // The platform reading lands in an atomic the JIT is not allowed to
        // fold away. A plain field store to a value nothing ever loads is dead
        // and can be removed, taking the clock read with it.
        private final AtomicLong sink = new AtomicLong();

        @Override
        public long nowNs() {
            sink.setRelease(SubMsTimer.nanosNow() - origin);
            return steps.addAndGet(STRIDE_NS);
        }
    }

    private static final class Measured {
        final long p50;
        final long p99;
        final double accept;

        Measured(long p50, long p99, double accept) {
            this.p50 = p50;
            this.p99 = p99;
            this.accept = accept;
        }
    }

    @FunctionalInterface
    private interface Op<T> {
        boolean apply(T target, int i);
    }

    private static long stat(SubMsPerfHarness h, String name, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals(name))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }

    /**
     * Builds the structure in {@code setup} OUTSIDE the timed region, warms on
     * a throwaway copy, then measures a FRESH one so the state at sample 0 is
     * identical at every sweep point - a warm pass on the measured instance
     * would leave its buckets drained and its counters bumped by however many
     * reps the time box happened to allow.
     *
     * <p>Two timed passes over that instance: one op per sample for the
     * published p99, then {@code batch} ops per sample for the p50 the
     * classifier reads. Pass {@code batch = 1} for an op already well clear of
     * the timer tick; the second pass is then skipped rather than run for
     * nothing.
     */
    private static <T> Measured measure(Supplier<T> setup, Op<T> op, int ops, int batch) {
        T warm = setup.get();
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int rep = 0; rep < WARM_MAX_REPS && System.nanoTime() < deadline; rep++) {
            op.apply(warm, rep % ops);
        }

        T target = setup.get();
        SubMsPerfHarness h = new SubMsPerfHarness("rate-limiter-feature", "java");
        long granted = 0L;
        boolean[] out = new boolean[1];
        SubMsPerfHarness.Stage st = h.stage("op", ops);
        for (int i = 0; i < ops; i++) {
            final int idx = i;
            st.time(() -> out[0] = op.apply(target, idx));
            if (out[0]) granted++;
        }
        long p99 = stat(h, "op", false);
        long p50;
        if (batch > 1) {
            int samples = ops / batch;
            SubMsPerfHarness.Stage bs = h.stage("batched", samples);
            for (int s = 0; s < samples; s++) {
                final int from = s * batch;
                bs.time(() -> {
                    for (int k = 0; k < batch; k++) {
                        op.apply(target, from + k);
                    }
                });
            }
            p50 = stat(h, "batched", true) / batch;
        } else {
            p50 = stat(h, "op", true);
        }
        return new Measured(p50, p99, (double) granted / ops);
    }

    /**
     * Sweeps, PRINTS the curve, fills the {@code (size, p50)} rows the
     * classifier reads and hands back the canonical (largest) point, whose p99
     * goes in the manifest. Printing the accept ratio alongside is what makes
     * it checkable that a sweep point did not quietly slide onto one branch.
     */
    private static Measured sweep(String label, int[] sizes, long[][] rows,
            IntFunction<Measured> at) {
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(':');
        Measured last = null;
        for (int i = 0; i < sizes.length; i++) {
            Measured m = at.apply(sizes[i]);
            rows[i][0] = sizes[i];
            rows[i][1] = m.p50;
            sb.append(" (n=").append(sizes[i]).append(" p50=").append(m.p50)
                    .append("ns p99=").append(m.p99).append("ns accept=")
                    .append(pct(m.accept)).append(')');
            last = m;
        }
        System.err.println(sb);
        return last;
    }

    private static String pct(double f) {
        return Math.round(f * 100.0) + "%";
    }

    private PerfFeaturesMain() {}
}
