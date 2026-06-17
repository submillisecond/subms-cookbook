package com.submillisecond.recipes.hdrhist;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.hdrhist.features.ConcurrentHdrHistogram;
import com.submillisecond.recipes.hdrhist.features.DecayingHdrHistogram;
import com.submillisecond.recipes.hdrhist.features.DualRecorder;
import com.submillisecond.recipes.hdrhist.features.HdrIterators;
import com.submillisecond.recipes.hdrhist.features.IterEntry;
import com.submillisecond.recipes.hdrhist.features.ManualClock;
import com.submillisecond.recipes.hdrhist.features.Merge;
import com.submillisecond.recipes.hdrhist.features.TaggedHdrHistogram;

import java.io.IOException;
import java.util.Iterator;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Runs the same 50k-entry workload against the base {@link HdrHistogram} plus
 * each feature variant (dual-recorder, concurrent-writes, merge, decay,
 * value-tagging, iterators), one stage per (variant, operation) with the SAME
 * stage names as the Rust bench so the cookbook FeaturePicker columns line up
 * across languages. JSON contract goes to stdout.
 *
 * <p>Every recorded value is drawn from an exponential-ish latency
 * distribution (most small, a long tail) so the bucket spread resembles real
 * latency capture rather than a flat sweep. The Rust side gates each variant
 * behind a Cargo feature; Java ships them all on the classpath, so every stage
 * is always emitted here.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.hdrhist.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final long SEED = 0;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("hdr-histogram-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Long.toString(SEED));
        h.meta("subms.recipe.slug", "subms-hdr-histogram");
        h.meta("subms.recipe.category", "observability");

        // ---------- base ----------
        {
            h.meta("subms.workload.feature", "base");
            HdrHistogram hist = new HdrHistogram(3);
            // Precompute the value stream so the per-element op is a pure array
            // read; record cost is content-independent so warming the live
            // histogram is safe.
            long[] values = latencies(ENTRIES);
            SubMsPerfHarness.Stage s = h.stage("base_record", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            s.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> hist.record(values[i % values.length]));
            // percentile is a whole-histogram O(buckets) idempotent read.
            SubMsPerfHarness.Stage sp = h.stage("base_percentile", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            sp.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> hist.valueAtPercentile(0.99));
        }

        // ---------- dual-recorder ----------
        {
            h.meta("subms.workload.feature", "dual-recorder");
            DualRecorder rec = new DualRecorder(3);
            long[] values = latencies(ENTRIES);
            SubMsPerfHarness.Stage s = h.stage("dual_recorder_record", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            s.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> rec.record(values[i % values.length]));
            // get_interval is an occasional consumer call (rotate + drain).
            // Each timed snapshot is preceded by untimed records that
            // warmThenTime can't interleave, so warm the rotate+drain path on a
            // throwaway recorder (drain cost is bucket-count bound, not content).
            DualRecorder warmRec = new DualRecorder(3);
            for (int i = 0; i < 2_000; i++) {
                warmRec.record(values[i % values.length]);
                warmRec.getIntervalHistogram();
            }
            final int intervals = 200;
            SubMsPerfHarness.Stage si = h.stage("dual_recorder_get_interval", intervals).withKind(SubMsStageKind.HOT_PATH);
            Lcg rng = new Lcg(SEED);
            for (int i = 0; i < intervals; i++) {
                for (int j = 0; j < 100; j++) rec.record(nextLatencyNs(rng));
                si.time(rec::getIntervalHistogram);
            }
        }

        // ---------- concurrent-writes ----------
        {
            h.meta("subms.workload.feature", "concurrent-writes");
            // Single-threaded per-op latency: the atomic increment cost
            // without cross-thread contention noise.
            ConcurrentHdrHistogram hist = new ConcurrentHdrHistogram(3);
            long[] values = latencies(ENTRIES);
            SubMsPerfHarness.Stage s = h.stage("concurrent_writes_record", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            s.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> hist.record(values[i % values.length]));
        }

        // ---------- merge ----------
        {
            h.meta("subms.workload.feature", "merge");
            // Build one populated histogram outside the timed loop, then time
            // the merge alone. Each merge folds src into a fresh copy-shaped
            // dst so the scan cost is what we measure.
            Lcg rng = new Lcg(SEED);
            HdrHistogram src = new HdrHistogram(3);
            for (int i = 0; i < ENTRIES; i++) src.record(nextLatencyNs(rng));
            // Each timed merge wants a fresh dst (untimed alloc warmThenTime
            // can't interleave); merge scans src (fixed), so warm the
            // addCountsFrom path on a reused throwaway dst, then time fresh ones.
            HdrHistogram warmDst = new HdrHistogram(3);
            for (int i = 0; i < 2_000; i++) Merge.merge(warmDst, src);
            final int merges = 2_000;
            SubMsPerfHarness.Stage s = h.stage("merge_merge", merges).withKind(SubMsStageKind.BATCH_OP);
            for (int i = 0; i < merges; i++) {
                HdrHistogram dst = new HdrHistogram(3);
                dst.record(1);
                s.time(() -> Merge.merge(dst, src));
            }
        }

        // ---------- decay ----------
        {
            h.meta("subms.workload.feature", "decay");
            // ManualClock advanced a fixed step per record so the decay
            // multiply actually fires (a system clock would mostly read zero
            // elapsed between adjacent records and skip the work).
            ManualClock clock = new ManualClock();
            long halflifeNs = 1_000_000_000L;
            DecayingHdrHistogram hist = new DecayingHdrHistogram(3, halflifeNs, clock);
            long[] values = latencies(ENTRIES);
            // record needs an untimed clock advance per call (so the decay
            // multiply fires) that warmThenTime can't interleave; warm the
            // record+decay path on a throwaway hist driven by its own clock.
            ManualClock warmClock = new ManualClock();
            DecayingHdrHistogram warmHist = new DecayingHdrHistogram(3, halflifeNs, warmClock);
            for (int i = 0; i < 20_000; i++) {
                warmClock.advanceNs(10_000);
                warmHist.record(values[i % values.length]);
            }
            SubMsPerfHarness.Stage s = h.stage("decay_record", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            Lcg rng = new Lcg(SEED);
            for (int i = 0; i < ENTRIES; i++) {
                long v = nextLatencyNs(rng);
                clock.advanceNs(10_000);
                s.time(() -> hist.record(v));
            }
            // percentile is a whole-histogram O(buckets) idempotent read.
            SubMsPerfHarness.Stage sp = h.stage("decay_percentile", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            sp.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> hist.valueAtPercentile(0.99));
        }

        // ---------- value-tagging ----------
        {
            h.meta("subms.workload.feature", "value-tagging");
            TaggedHdrHistogram hist = new TaggedHdrHistogram(3);
            // Precompute the (value, tag) stream so the per-element op is a pair
            // of array reads; record cost is content-independent.
            Lcg rng = new Lcg(SEED);
            long[] values = new long[ENTRIES];
            byte[] tags = new byte[ENTRIES];
            for (int i = 0; i < ENTRIES; i++) {
                values[i] = nextLatencyNs(rng);
                tags[i] = (byte) (Integer.remainderUnsigned(rng.nextU32(), 8));
            }
            SubMsPerfHarness.Stage s = h.stage("value_tagging_record", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            s.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> hist.record(values[i % values.length], tags[i % tags.length]));
            // per-tag percentile is a whole-histogram O(buckets) idempotent read.
            SubMsPerfHarness.Stage sp = h.stage("value_tagging_per_tag_percentile", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            sp.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> hist.valueAtPercentileForTag(0.99, (byte) (i % 8)));
        }

        // ---------- iterators ----------
        {
            h.meta("subms.workload.feature", "iterators");
            // Populate one histogram, then step the linear iterator over it.
            // Each next() advances to the next populated bucket; we time
            // individual steps across many full passes to fill ENTRIES.
            HdrHistogram hist = new HdrHistogram(3);
            Lcg rng = new Lcg(SEED);
            for (int i = 0; i < ENTRIES; i++) hist.record(nextLatencyNs(rng));
            // next() is stateful (advances a cursor) and the iterator must be
            // recreated when exhausted, which warmThenTime can't drive; warm the
            // stepping path with full passes over the populated histogram.
            int warmed = 0;
            while (warmed < 20_000) {
                Iterator<IterEntry> wit = HdrIterators.linear(hist);
                while (wit.hasNext()) {
                    wit.next();
                    if (++warmed >= 20_000) break;
                }
            }
            SubMsPerfHarness.Stage s = h.stage("iterators_next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            int emitted = 0;
            while (emitted < ENTRIES) {
                Iterator<IterEntry> it = HdrIterators.linear(hist);
                while (it.hasNext()) {
                    s.time(it::next);
                    emitted++;
                    if (emitted >= ENTRIES) break;
                }
            }
        }

        h.writeJson(System.out);
    }

    /**
     * Exponential-ish latency in nanoseconds: a base floor plus a value whose
     * magnitude follows -ln(u) so most samples are small and a thin tail
     * stretches into the microseconds. Deterministic under {@link #SEED}.
     */
    private static long nextLatencyNs(Lcg rng) {
        double u = (Integer.toUnsignedLong(rng.nextU32()) + 1.0) / (0xFFFFFFFFL + 2.0);
        double v = -(Math.log(u)) * 2_000.0;
        return (long) v + 50;
    }

    /** Deterministic latency stream from a fresh seeded LCG, for per-element
     *  stages whose timed op must be a pure array read under warmThenTime. */
    private static long[] latencies(int n) {
        Lcg rng = new Lcg(SEED);
        long[] out = new long[n];
        for (int i = 0; i < n; i++) out[i] = nextLatencyNs(rng);
        return out;
    }

    /** Deterministic LCG matching the central {@code subms::SubMsLcg}. */
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed | 1L;
        }

        int nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> 32);
        }
    }
}
