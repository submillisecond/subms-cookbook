package com.submillisecond.recipes.arena;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.arena.features.AlignedArena;
import com.submillisecond.recipes.arena.features.FreelistArena;
import com.submillisecond.recipes.arena.features.GrowableArena;
import com.submillisecond.recipes.arena.features.StatsArena;
import com.submillisecond.recipes.arena.features.TypedArena;

import java.io.IOException;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Runs a 50k-iteration arena workload against the fixed-capacity base bump
 * arena, plus each opt-in feature variant (typed, growable, stats, aligned,
 * freelist). One stage block per variant - base_allocate, base_reset,
 * typed_allocate, growable_allocate, etc. - with the SAME stage names as the
 * Rust bench so the cookbook FeaturePicker columns line up across languages.
 * JSON contract goes to stdout.
 *
 * <p>The workload mirrors the per-request reuse pattern the arena is built
 * for: allocate a batch of fixed-layout values, then reset() to rewind the
 * cursor, repeating until ITERATIONS allocations have been timed. The batch
 * size stays well inside each arena's capacity so the base (fixed-capacity,
 * not auto-growing) never overflows. The growable stage deliberately sizes
 * the initial chunk so the batch crosses a grow boundary, exercising the
 * chunk-allocation path the other variants skip.
 *
 * <p>The Rust bench gates each feature behind a Cargo feature; the Java
 * artefact ships every feature in one jar, so all variants always emit.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.arena.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ITERATIONS = 50_000;
    private static final long SEED = 0L;

    // Allocate this many values between resets. 256 longs = 2 KiB, comfortably
    // inside the base's 4 KiB chunk so the fixed-capacity arena never refuses.
    private static final int BATCH = 256;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("arena-allocator-features", "java");
        h.input("iterations", Integer.toString(ITERATIONS));
        h.input("seed", Long.toString(SEED));
        h.input("batch", Integer.toString(BATCH));
        h.meta("subms.recipe.slug", "subms-arena-allocator");
        h.meta("subms.recipe.category", "memory");

        base(h);
        typed(h);
        growable(h);
        stats(h);
        aligned(h);
        freelist(h);

        h.writeJson(System.out);
    }

    // ---------- base ----------
    // Fixed-capacity single-chunk arena: allocate a batch, reset, repeat. The
    // reset stage is sampled once per batch (a constant-time cursor rewind).
    private static void base(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "base");
        BumpArena a = new BumpArena(4096);
        // Warm the allocate path on a throwaway: the fixed-capacity buffer
        // exhausts after ~512 allocs, so we cannot warmThenTime on the real
        // arena (warmup would overflow before the reset cadence kicks in).
        warmAllocate(new BumpArena(4096));
        SubMsPerfHarness.Stage allocStage = h.stage("base_allocate", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) {
            allocStage.time(() -> a.allocate(8, 8));
            if ((i + 1) % BATCH == 0) a.reset();
        }
        SubMsPerfHarness.Stage resetStage = h.stage("base_reset", ITERATIONS / BATCH + 1).withKind(SubMsStageKind.HOT_PATH);
        a.allocate(8, 8);
        // reset is an idempotent O(1) cursor rewind (cursor = 0), so repeated
        // bare resets measure the same instruction and warmThenTime is safe.
        resetStage.warmThenTime(500, ITERATIONS / BATCH, a::reset);
    }

    // Drive the bump-allocate path enough to reach C2 without overflowing a
    // fixed-capacity buffer (reset before each batch fills the chunk).
    private static void warmAllocate(BumpArena a) {
        for (int i = 0; i < 20_000; i++) {
            a.allocate(8, 8);
            if ((i + 1) % BATCH == 0) a.reset();
        }
    }

    // ---------- typed ----------
    // TypedArena<long[]> as the heap-object parallel of Rust's TypedArena<u64>:
    // allocate to capacity, reset, repeat. The single-element long[] is the
    // mutable u64 slot the caller writes through.
    private static void typed(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "typed");
        TypedArena<long[]> a = new TypedArena<>(BATCH, () -> new long[1]);
        // Capacity == BATCH, so the alloc path overflows without the reset
        // cadence: warm on a throwaway that resets per batch, time the real one.
        warmTypedAllocate(new TypedArena<>(BATCH, () -> new long[1]));
        long counter = 0L;
        SubMsPerfHarness.Stage allocStage = h.stage("typed_allocate", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) {
            counter++;
            long c = counter;
            allocStage.time(() -> a.allocate()[0] = c);
            if ((i + 1) % BATCH == 0) a.reset();
        }
        SubMsPerfHarness.Stage resetStage = h.stage("typed_reset", ITERATIONS / BATCH + 1).withKind(SubMsStageKind.HOT_PATH);
        a.reset();
        a.allocate();
        // reset is an idempotent len = 0 rewind, so warmThenTime is safe.
        resetStage.warmThenTime(500, ITERATIONS / BATCH, a::reset);
    }

    private static void warmTypedAllocate(TypedArena<long[]> a) {
        for (int i = 0; i < 20_000; i++) {
            a.allocate()[0] = i;
            if ((i + 1) % BATCH == 0) a.reset();
        }
    }

    // ---------- growable ----------
    // Auto-grow arena: size the initial chunk so a BATCH crosses a grow
    // boundary, exercising the chunk-allocation path. reset() keeps the largest
    // chunk so steady-state batches settle on a single chunk.
    private static void growable(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "growable");
        // 512-byte initial chunk = 64 longs; a 256-batch forces grows on the
        // first batch, then reset() retains the grown chunk for steady state.
        GrowableArena a = new GrowableArena(512);
        // Warm on a throwaway: warming the live arena without the reset cadence
        // would grow it past the intended grow-once-then-steady shape the
        // measured loop relies on.
        warmGrowableAllocate(new GrowableArena(512));
        SubMsPerfHarness.Stage allocStage = h.stage("growable_allocate", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) {
            allocStage.time(() -> a.allocate(8, 8));
            if ((i + 1) % BATCH == 0) a.reset();
        }
        SubMsPerfHarness.Stage resetStage = h.stage("growable_reset", ITERATIONS / BATCH + 1).withKind(SubMsStageKind.HOT_PATH);
        a.reset();
        a.allocate(8, 8);
        // reset is an idempotent cursor rewind, so warmThenTime is safe.
        resetStage.warmThenTime(500, ITERATIONS / BATCH, a::reset);
    }

    private static void warmGrowableAllocate(GrowableArena a) {
        for (int i = 0; i < 20_000; i++) {
            a.allocate(8, 8);
            if ((i + 1) % BATCH == 0) a.reset();
        }
    }

    // ---------- stats ----------
    // Instrumented arena: allocate (counter writes per call) + snapshot the
    // live Stats. snapshot() is a struct copy, sampled per allocation.
    private static void stats(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "stats");
        StatsArena a = new StatsArena(4096);
        // Warm on a throwaway: warming the live arena would inflate its lifetime
        // counters and grow the buffer before the measured loop runs.
        warmStatsAllocate(new StatsArena(4096));
        SubMsPerfHarness.Stage allocStage = h.stage("stats_allocate", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) {
            allocStage.time(() -> a.allocate(8, 8));
            if ((i + 1) % BATCH == 0) a.reset();
        }
        // snapshot is an idempotent record copy with no capacity, so warm the
        // live arena directly.
        SubMsPerfHarness.Stage snapStage = h.stage("stats_snapshot", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        snapStage.warmThenTime(Math.min(ITERATIONS, 20_000), ITERATIONS, (int i) -> blackHole(a.stats()));
    }

    private static void warmStatsAllocate(StatsArena a) {
        for (int i = 0; i < 20_000; i++) {
            a.allocate(8, 8);
            if ((i + 1) % BATCH == 0) a.reset();
        }
    }

    // ---------- aligned ----------
    // Cache-line allocations: allocAligned(64, 64) repeatedly, reset per batch.
    private static void aligned(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "aligned");
        // 64-byte slots: a 256-batch is 16 KiB, so size the chunk for the batch.
        int chunk = BATCH * 64 + 64;
        AlignedArena a = new AlignedArena(chunk);
        // Fixed-capacity: warm on a throwaway of the same size so warmup does
        // not exhaust the live buffer before the reset cadence engages.
        warmAlignedAllocate(new AlignedArena(chunk));
        SubMsPerfHarness.Stage allocStage = h.stage("aligned_allocate_aligned", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) {
            allocStage.time(() -> blackHole(a.allocAligned(64, 64)));
            if ((i + 1) % BATCH == 0) a.reset();
        }
    }

    private static void warmAlignedAllocate(AlignedArena a) {
        for (int i = 0; i < 20_000; i++) {
            a.allocAligned(64, 64);
            if ((i + 1) % BATCH == 0) a.reset();
        }
    }

    // ---------- freelist ----------
    // Per-object reuse: alloc a slot, release it, alloc again (reuse hit). The
    // free + reuse pair is the steady-state object-pool shape this variant
    // targets, so both the allocate (reuse path) and free stages get sampled.
    private static void freelist(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "freelist");
        FreelistArena<long[]> a = new FreelistArena<>(2, () -> new long[1]);
        // Each timed op pairs with an untimed release that warmThenTime can't
        // interleave (alloc-only would exhaust capacity 2), so warm the
        // reuse-hit + release paths on a throwaway, then time the live loops.
        warmFreelist(new FreelistArena<>(2, () -> new long[1]));

        // Prime one slot so the very first timed alloc hits the freelist.
        long[] primed = a.allocate();
        a.release(primed);

        SubMsPerfHarness.Stage allocStage = h.stage("freelist_allocate", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < ITERATIONS; i++) {
            long[] held = timedAllocate(allocStage, a);
            // Return it so the next iteration reuses the same slot. Timed
            // separately below; here we just keep the freelist warm.
            a.release(held);
        }

        // Re-prime, then time the free path on its own.
        long[] p = a.allocate();
        SubMsPerfHarness.Stage freeStage = h.stage("freelist_free", ITERATIONS).withKind(SubMsStageKind.HOT_PATH);
        long[] cur = p;
        for (int i = 0; i < ITERATIONS; i++) {
            long[] toFree = cur;
            freeStage.time(() -> a.release(toFree));
            // Pull it back out (reuse hit, untimed) so the next free has a slot.
            cur = a.allocate();
        }
    }

    private static long[] timedAllocate(SubMsPerfHarness.Stage stage, FreelistArena<long[]> a) {
        long[][] out = new long[1][];
        stage.time(() -> out[0] = a.allocate());
        return out[0];
    }

    // Drive the alloc-reuse and release paths to C2 (capacity 2, so the pair
    // keeps the bucket non-empty without exhausting it).
    private static void warmFreelist(FreelistArena<long[]> a) {
        long[] held = a.allocate();
        a.release(held);
        for (int i = 0; i < 20_000; i++) {
            long[] x = a.allocate();
            a.release(x);
        }
    }

    @SuppressWarnings("unused")
    private static void blackHole(Object o) {
        if (o == SINK) System.out.print("");
    }

    private static final Object SINK = new Object();
}
