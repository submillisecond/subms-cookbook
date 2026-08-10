package com.submillisecond.recipes.mergeiter;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.mergeiter.features.DedupEntry;
import com.submillisecond.recipes.mergeiter.features.DedupMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PriorityEntry;
import com.submillisecond.recipes.mergeiter.features.PriorityMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PrioritySource;
import com.submillisecond.recipes.mergeiter.features.ReverseMergeIterator;
import com.submillisecond.recipes.mergeiter.features.SeekableMergeIterator;
import com.submillisecond.recipes.mergeiter.features.TombstoneEntry;
import com.submillisecond.recipes.mergeiter.features.TombstoneMergeIterator;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.Supplier;

/**
 * Feature classification bench, the Java mirror of
 * {@code rust/examples/perf_features.rs}. Each feature's representative op is
 * swept across three input sizes, {@link SubMsFeatureManifest#classify} DECIDES
 * the category from the shape of that sweep, and the decision plus a measured
 * p99-by-stage is merge-written into {@code ../.subms/features/java.json}.
 *
 * <p>The sweep axis is the TOTAL number of elements across the 16 merged
 * streams. A k-way merge step is O(log k) in the number of streams and
 * independent of how many elements sit behind them, so a per-element feature
 * should read flat and the sweep is here to prove that rather than assume it.
 *
 * <p>Two measurement decisions carry the whole file:
 *
 * <ul>
 *   <li>A measurement TIMES A FIXED NUMBER OF ELEMENTS however large the input
 *       is. Timing a whole drain would report the size of the ANSWER: 64x the
 *       elements take 64x as long at an unchanged per-element cost, and every
 *       feature would classify structural. The drain still visits every
 *       element, so the working set grows with the sweep, but only
 *       {@code SAMPLES} batches of it are timed.
 *   <li>Elements are timed in BATCHES of {@code BATCH} and the recorded figure
 *       is the batch mean. A merge step costs tens of ns and the platform clock
 *       ticks at 100 ns, so an unbatched sample reads 0 or 100 ns and the p50
 *       of every variant, base included, comes back as exactly one tick.
 *       Unbatched, this bench measures the clock.
 * </ul>
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category
 * is an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.mergeiter.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {

    /**
     * Total elements across all streams: a 64x span. The bottom of the range is
     * already past the point where the 16-entry heap fits in L1 with room to
     * spare, so a per-element cost that still climbed would be a real cache
     * effect rather than a fixed per-call cost being amortised away.
     */
    private static final int[] SIZES = {32_768, 262_144, 2_097_152};
    private static final int CANON = SIZES[SIZES.length - 1];
    private static final int STREAMS = 16;

    /** Elements per timed sample; the recorded value is the batch mean. */
    private static final int BATCH = 64;
    /**
     * Timed batches per measurement. Fixed across the sweep, so the statistic
     * is the cost of ONE element and not the length of the drain.
     */
    private static final int SAMPLES = 512;
    /**
     * A short input runs out of batches before it runs out of samples, so the
     * drain repeats over a freshly built iterator until the sample count is
     * met. Without it the smallest sweep point was decided by a quarter of the
     * samples of the largest, and a single GC blip moved it by 30%.
     */
    private static final int MAX_PASSES = 16;

    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_MAX_REPS = 64;

    /**
     * Keys skipped per seek. Held CONSTANT across the sweep for the same reason
     * the timed element count is: seek walks each stream forward one entry at a
     * time until it reaches the target, so its cost is set by the skip
     * distance, not by how many elements sit beyond it. Spreading a fixed
     * number of seeks over a growing key range would sweep the skip distance
     * and call it size.
     */
    private static final long SEEK_SKIP = 64;
    /** Seeks per pass, capped so a pass consumes at most half the smallest input. */
    private static final int SEEK_ROUNDS = 256;
    /**
     * Seeks per timed sample. A seek over 64 keys costs a few hundred ns, which
     * is three or four clock ticks, and the unbatched curve jittered by a full
     * tick between sweep points.
     */
    private static final int SEEK_BATCH = 2;
    private static final int SEEK_NEXT_ROUNDS = 128;
    /**
     * Passes over a freshly built iterator. One pass cannot yield SAMPLES
     * without running the skip distance up with it, and the skip distance is
     * the one thing this measurement holds fixed. Kept as low as the sample
     * count allows: a pass builds the whole n-element input to seek over the
     * first 16k of it, and that garbage is what a later measurement collects.
     */
    private static final int SEEK_PASSES = 4;

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: a plain merge step with no feature enabled. Every
        // feature decorates this step, so it is what they are classified
        // against. Swept as well as measured, because a base that itself
        // drifted with size would make every feature's flat reading
        // meaningless.
        long[][] baseSweep = sweep("base/next",
                n -> perElement(() -> new MergeIterator<>(plainStreams(n)), n, true));
        long baseP50 = baseSweep[baseSweep.length - 1][1];
        System.err.println("base next p50: " + baseP50 + "ns/element");

        seekTo(manifest, baseP50);
        reverse(manifest, baseP50);
        tombstones(manifest, baseP50);
        dedup(manifest, baseP50);
        priority(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- seek-to: skip forward past a key ----------
    private static void seekTo(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("seek-to/seek", n -> seekOnly(n, true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("seek", seekOnly(CANON, false));
        p99.put("next_after_seek", seekThenNext(CANON, false));
        manifest.setFeature("seek-to", dec.category(), p99, dec.reason());
    }

    // ---------- reverse: descending merge + seekForPrev ----------
    private static void reverse(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("reverse/next", n -> perElement(
                () -> new ReverseMergeIterator<>(descendingStreams(n)), n, true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("reverse_next", perElement(
                () -> new ReverseMergeIterator<>(descendingStreams(CANON)), CANON, false));
        p99.put("seek_for_prev", seekForPrevOnly(CANON, false));
        manifest.setFeature("reverse", dec.category(), p99, dec.reason());
    }

    // ---------- tombstones: delete markers mask same-key entries ----------
    private static void tombstones(SubMsFeatureManifest manifest, long baseP50) {
        // Every 8th key is a tombstone, so one next in eight pops twice and
        // loops to find the next live key. The decoration is per element.
        long[][] sw = sweep("tombstones/next", n -> perElement(
                () -> new TombstoneMergeIterator<>(tombstoneStreams(n)), n / 8 * 7, true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("tombstones_next", perElement(
                () -> new TombstoneMergeIterator<>(tombstoneStreams(CANON)), CANON / 8 * 7, false));
        manifest.setFeature("tombstones", dec.category(), p99, dec.reason());
    }

    // ---------- dedup: collapse equal keys, latest source wins ----------
    private static void dedup(SubMsFeatureManifest manifest, long baseP50) {
        // Halved key space, so every key is carried by two sources and every
        // next pops twice: the collapse path runs on every element yielded.
        long[][] sw = sweep("dedup/next", n -> perElement(
                () -> new DedupMergeIterator<>(dedupStreams(n)), n / 2, true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("dedup_next", perElement(
                () -> new DedupMergeIterator<>(dedupStreams(CANON)), CANON / 2, false));
        manifest.setFeature("dedup", dec.category(), p99, dec.reason());
    }

    // ---------- priority: explicit per-source precedence on key tie ----------
    private static void priority(SubMsFeatureManifest manifest, long baseP50) {
        // Same collide-on-halved-keys shape as dedup, plus a priority field in
        // the heap comparison, so the two figures are directly comparable.
        long[][] sw = sweep("priority/next", n -> perElement(
                () -> new PriorityMergeIterator<>(prioritySources(n)), n / 2, true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("priority_next", perElement(
                () -> new PriorityMergeIterator<>(prioritySources(CANON)), CANON / 2, false));
        manifest.setFeature("priority", dec.category(), p99, dec.reason());
    }

    // ---------- harness plumbing ----------

    /**
     * Sweeps and PRINTS the curve. A ratio-compressed or non-monotonic sweep
     * classifies flat, and the only way to catch one is to read the rows.
     */
    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = at.at(SIZES[i]);
            sb.append('(').append(rows[i][0]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n);
    }

    /**
     * Runs the measurement into a throwaway harness until the budget expires,
     * then once more into the harness whose numbers are kept.
     *
     * <p>Warming the WORK is not enough: warm has to run the identical TIMED
     * path. With warm draining the iterator untimed, the first measurement a
     * process made still read about 20% high, and it stayed 20% high with the
     * sweep reversed - so it was the first entry into the timed region, not the
     * size. Time-boxed rather than a fixed rep count, or a cheap size gets the
     * same handful of passes as an expensive one.
     */
    private static long warmed(Consumer<SubMsPerfHarness> measure, boolean median) {
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int i = 0; i < WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            SubMsPerfHarness scratch = harness();
            measure.accept(scratch);
            stat(scratch, median);
        }
        // Warm and setup build multi-megabyte inputs and throw them away.
        // Collecting here charges that garbage to the warm instead of to the
        // timed region the collector would otherwise have run in: an
        // uncollected backlog pushed one sweep point to 8x its neighbours.
        System.gc();
        SubMsPerfHarness h = harness();
        measure.accept(h);
        return stat(h, median);
    }

    /**
     * Per-element cost of a merge step, in ns. The supplier builds a fresh
     * iterator OUTSIDE the timed region (a merge iterator is single-use, so the
     * input has to be rebuilt per pass and building it is not the thing being
     * measured). The drain consumes the whole input; every stride-th batch is
     * timed, which keeps the sample count fixed while the working set grows.
     */
    private static <T> long perElement(Supplier<Iterator<T>> make, long expectedOut,
            boolean median) {
        long stride = Math.max(1, expectedOut / ((long) BATCH * SAMPLES));
        return warmed(h -> {
            SubMsPerfHarness.Stage st = h.stage("op", SAMPLES + 1);
            int recorded = 0;
            for (int pass = 0; pass < MAX_PASSES && recorded < SAMPLES; pass++) {
                Iterator<T> it = make.get();
                long batch = 0;
                while (true) {
                    int taken = 0;
                    if (batch % stride == 0) {
                        SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                        while (taken < BATCH && it.hasNext()) {
                            it.next();
                            taken++;
                        }
                        long ns = t0.elapsedNs();
                        if (taken == BATCH) {
                            st.record(ns / BATCH);
                            recorded++;
                        }
                    } else {
                        while (taken < BATCH && it.hasNext()) {
                            it.next();
                            taken++;
                        }
                    }
                    if (taken < BATCH) {
                        break;
                    }
                    batch++;
                }
            }
        }, median);
    }

    private static long seekOnly(int n, boolean median) {
        Long[] targets = seekTargets(SEEK_ROUNDS, SEEK_SKIP);
        return warmed(h -> {
            SubMsPerfHarness.Stage st = h.stage("op", SEEK_PASSES * SEEK_ROUNDS / SEEK_BATCH + 1);
            for (int p = 0; p < SEEK_PASSES; p++) {
                SeekableMergeIterator<Long> it = new SeekableMergeIterator<>(plainStreams(n));
                int r = 0;
                while (r < SEEK_ROUNDS) {
                    SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                    for (int b = 0; b < SEEK_BATCH; b++) {
                        it.seek(targets[r]);
                        r++;
                    }
                    st.record(t0.elapsedNs() / SEEK_BATCH);
                }
            }
        }, median);
    }

    /**
     * Streaming cost of the next calls that follow a seek. Batched for the same
     * clock-tick reason as every other per-element figure, so the first element
     * of each batch is the one that lands right after the seek.
     */
    private static long seekThenNext(int n, boolean median) {
        Long[] targets = seekTargets(SEEK_NEXT_ROUNDS, SEEK_SKIP + BATCH);
        return warmed(h -> {
            SubMsPerfHarness.Stage st = h.stage("op", SEEK_PASSES * SEEK_NEXT_ROUNDS + 1);
            for (int p = 0; p < SEEK_PASSES; p++) {
                SeekableMergeIterator<Long> it = new SeekableMergeIterator<>(plainStreams(n));
                for (Long target : targets) {
                    it.seek(target);
                    SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                    int taken = 0;
                    while (taken < BATCH && it.hasNext()) {
                        it.next();
                        taken++;
                    }
                    long ns = t0.elapsedNs();
                    if (taken == BATCH) {
                        st.record(ns / BATCH);
                    }
                }
            }
        }, median);
    }

    /**
     * Mirror of {@link #seekOnly}, walking backward. Same fixed skip distance,
     * so the two seek figures are directly comparable.
     */
    private static long seekForPrevOnly(int n, boolean median) {
        Long[] targets = new Long[SEEK_ROUNDS];
        for (int r = 0; r < SEEK_ROUNDS; r++) {
            targets[r] = Math.max(0L, (n - 1) - (r + 1) * SEEK_SKIP);
        }
        return warmed(h -> {
            SubMsPerfHarness.Stage st = h.stage("op", SEEK_PASSES * SEEK_ROUNDS / SEEK_BATCH + 1);
            for (int p = 0; p < SEEK_PASSES; p++) {
                ReverseMergeIterator<Long> it = new ReverseMergeIterator<>(descendingStreams(n));
                int r = 0;
                while (r < SEEK_ROUNDS) {
                    SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                    for (int b = 0; b < SEEK_BATCH; b++) {
                        it.seekForPrev(targets[r]);
                        r++;
                    }
                    st.record(t0.elapsedNs() / SEEK_BATCH);
                }
            }
        }, median);
    }

    /** Boxed up front: an autobox inside the timed region is not a seek cost. */
    private static Long[] seekTargets(int rounds, long skip) {
        Long[] targets = new Long[rounds];
        for (int r = 0; r < rounds; r++) {
            targets[r] = (r + 1) * skip;
        }
        return targets;
    }

    private static SubMsPerfHarness harness() {
        return new SubMsPerfHarness("merge-iterator-feature", "java");
    }

    private static long stat(SubMsPerfHarness h, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }

    // ---------- inputs ----------

    /**
     * Stream {@code s} carries values {@code s, s+STREAMS, s+2*STREAMS, ...} so
     * the 16 streams interleave into the dense range {@code 0..n} with no gaps.
     */
    private static List<Iterator<Long>> plainStreams(int n) {
        int per = n / STREAMS;
        List<Iterator<Long>> streams = new ArrayList<>(STREAMS);
        for (int s = 0; s < STREAMS; s++) {
            List<Long> values = new ArrayList<>(per);
            for (int i = 0; i < per; i++) {
                values.add((long) (s + i * STREAMS));
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    /**
     * The {@link #plainStreams} shape reversed: stream {@code s} counts DOWN,
     * so the 16 streams interleave into a dense descending {@code n..0}.
     */
    private static List<Iterator<Long>> descendingStreams(int n) {
        int per = n / STREAMS;
        List<Iterator<Long>> streams = new ArrayList<>(STREAMS);
        for (int s = 0; s < STREAMS; s++) {
            List<Long> values = new ArrayList<>(per);
            for (int i = 0; i < per; i++) {
                values.add((long) (s + (per - 1 - i) * STREAMS));
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    private static List<Iterator<TombstoneEntry<Long, Long>>> tombstoneStreams(int n) {
        int per = n / STREAMS;
        List<Iterator<TombstoneEntry<Long, Long>>> streams = new ArrayList<>(STREAMS);
        for (int s = 0; s < STREAMS; s++) {
            List<TombstoneEntry<Long, Long>> values = new ArrayList<>(per);
            for (int i = 0; i < per; i++) {
                long key = s + (long) i * STREAMS;
                values.add(key % 8 == 0
                        ? TombstoneEntry.tombstone(key)
                        : TombstoneEntry.live(key, key));
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    private static List<Iterator<DedupEntry<Long, Long>>> dedupStreams(int n) {
        int per = n / STREAMS;
        List<Iterator<DedupEntry<Long, Long>>> streams = new ArrayList<>(STREAMS);
        for (int s = 0; s < STREAMS; s++) {
            List<DedupEntry<Long, Long>> values = new ArrayList<>(per);
            for (int i = 0; i < per; i++) {
                long key = (s + (long) i * STREAMS) / 2;
                values.add(new DedupEntry<>(key, key));
            }
            streams.add(values.iterator());
        }
        return streams;
    }

    private static List<PrioritySource<Long, Long>> prioritySources(int n) {
        int per = n / STREAMS;
        List<PrioritySource<Long, Long>> sources = new ArrayList<>(STREAMS);
        for (int s = 0; s < STREAMS; s++) {
            List<PriorityEntry<Long, Long>> values = new ArrayList<>(per);
            for (int i = 0; i < per; i++) {
                long key = (s + (long) i * STREAMS) / 2;
                values.add(new PriorityEntry<>(key, key));
            }
            sources.add(new PrioritySource<>(STREAMS - s, values.iterator()));
        }
        return sources;
    }

    private PerfFeaturesMain() {}
}
