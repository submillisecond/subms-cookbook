package com.submillisecond.recipes.ratelimit;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.ratelimit.features.DistributedLimiter;
import com.submillisecond.recipes.ratelimit.features.HierarchicalLimiter;
import com.submillisecond.recipes.ratelimit.features.InMemoryBackend;
import com.submillisecond.recipes.ratelimit.features.MeteredTokenBucket;
import com.submillisecond.recipes.ratelimit.features.TokenBucket;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per feature variant - base_try_acquire,
 * token_bucket_try_acquire, hierarchical_try_acquire,
 * distributed_backend_try_acquire, metrics_try_acquire - with the SAME
 * stage names as the Rust bench so the cookbook FeaturePicker columns
 * line up across languages. JSON contract goes to stdout.
 *
 * <p>Every limiter is sized so the grant path dominates: capacity / burst
 * comfortably exceeds the iteration count, so we measure the cost of a
 * successful {@code tryAcquire} (the hot path) rather than the reject path.
 * The feature limiters all run on a real {@code SystemClock} (their default
 * constructors), mirroring the Rust bench.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.ratelimit.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ITERATIONS = 50_000;
    private static final long SEED = 0L;
    private static final int WARMUP = 20_000;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("rate-limiter-features", "java");
        h.input("iterations", Integer.toString(ITERATIONS));
        h.input("seed", Long.toString(SEED));
        h.meta("subms.recipe.slug", "subms-rate-limiter");
        h.meta("subms.recipe.category", "scheduling");

        // ---------- base ----------
        h.meta("subms.workload.feature", "base");
        // High rate + a burst headroom larger than the loop so every acquire
        // is granted. The base limiter pushes tat forward by one period per
        // grant; with wall-clock now near-static across a tight loop, the
        // burst window must cover the whole run.
        //
        // Warmup runs on a throwaway limiter: acquiring on the real one would
        // consume burst budget, so the measured loop owns a fresh limiter and
        // the throwaway absorbs the JIT-warming grants.
        RateLimiter baseWarm = new RateLimiter(1_000_000.0, 2L * ITERATIONS);
        for (int i = 0; i < WARMUP; i++) baseWarm.tryAcquire();
        RateLimiter base = new RateLimiter(1_000_000.0, 2L * ITERATIONS);
        SubMsPerfHarness.Stage baseStage = h.stage("base_try_acquire", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) baseStage.time(base::tryAcquire);

        // ---------- token-bucket ----------
        h.meta("subms.workload.feature", "token-bucket");
        // Capacity above the iteration count + a high refill so the bucket
        // never empties during the run. Warm on a throwaway so the measured
        // bucket starts at full capacity.
        TokenBucket tbWarm = new TokenBucket(2L * ITERATIONS, 1_000_000.0);
        for (int i = 0; i < WARMUP; i++) tbWarm.tryAcquire(1);
        TokenBucket tb = new TokenBucket(2L * ITERATIONS, 1_000_000.0);
        SubMsPerfHarness.Stage tbStage = h.stage("token_bucket_try_acquire", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) tbStage.time(() -> tb.tryAcquire(1));

        // ---------- hierarchical ----------
        h.meta("subms.workload.feature", "hierarchical");
        // Parent + single child both sized above the loop so each call
        // clears child AND parent. Each grant drains both buckets, so warm on
        // a throwaway to keep the measured buckets full.
        HierarchicalLimiter hierWarm = new HierarchicalLimiter(
                2L * ITERATIONS, 1_000_000.0, 1, 2L * ITERATIONS, 1_000_000.0);
        for (int i = 0; i < WARMUP; i++) hierWarm.tryAcquire(0, 1);
        HierarchicalLimiter hier = new HierarchicalLimiter(
                2L * ITERATIONS,
                1_000_000.0,
                1,
                2L * ITERATIONS,
                1_000_000.0);
        SubMsPerfHarness.Stage hierStage = h.stage("hierarchical_try_acquire", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) hierStage.time(() -> hier.tryAcquire(0, 1));

        // ---------- distributed-backend ----------
        h.meta("subms.workload.feature", "distributed-backend");
        // Fixed-window counter; limit above the iteration count and a wide
        // window so all calls land inside one window and grant. Warm on a
        // throwaway backend + limiter so the measured window counter starts at
        // zero (bumping the real key would eat into the limit).
        DistributedLimiter distWarm = new DistributedLimiter(
                new InMemoryBackend(), 2L * ITERATIONS, 3_600_000_000_000L);
        for (int i = 0; i < WARMUP; i++) distWarm.tryAcquire("hot-key");
        DistributedLimiter dist = new DistributedLimiter(
                new InMemoryBackend(),
                2L * ITERATIONS,
                3_600_000_000_000L);
        SubMsPerfHarness.Stage distStage = h.stage("distributed_backend_try_acquire", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) distStage.time(() -> dist.tryAcquire("hot-key"));

        // ---------- metrics ----------
        h.meta("subms.workload.feature", "metrics");
        // Metered wrapper over a token bucket; warm on a throwaway so the
        // measured bucket starts full.
        MeteredTokenBucket meteredWarm = new MeteredTokenBucket(2L * ITERATIONS, 1_000_000.0);
        for (int i = 0; i < WARMUP; i++) meteredWarm.tryAcquire(1);
        MeteredTokenBucket metered = new MeteredTokenBucket(2L * ITERATIONS, 1_000_000.0);
        SubMsPerfHarness.Stage metStage = h.stage("metrics_try_acquire", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) metStage.time(() -> metered.tryAcquire(1));

        h.writeJson(System.out);
    }
}
