package com.submillisecond.recipes.hdrhist;

import com.submillisecond.recipes.hdrhist.features.ConcurrentHdrHistogram;
import com.submillisecond.recipes.hdrhist.features.DecayingHdrHistogram;
import com.submillisecond.recipes.hdrhist.features.DualRecorder;
import com.submillisecond.recipes.hdrhist.features.HdrIterators;
import com.submillisecond.recipes.hdrhist.features.IterEntry;
import com.submillisecond.recipes.hdrhist.features.ManualClock;
import com.submillisecond.recipes.hdrhist.features.Merge;
import com.submillisecond.recipes.hdrhist.features.Snapshot;
import com.submillisecond.recipes.hdrhist.features.TaggedHdrHistogram;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Sample app: a tour of {@code subms-hdr-histogram}, base API first, then each
 * optional feature. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.hdrhist.SampleApp}
 *
 * <ul>
 *   <li>base              - tick-to-trade latency capture with p50/p99/p999 reads
 *       plus Gil Tene coordinated-omission correction
 *   <li>concurrent-writes - many feed-handler threads recording into one histogram
 *   <li>dual-recorder     - lock-free interval percentile reporting
 *   <li>merge             - roll per-shard histograms into a fleet-wide view
 *   <li>decay             - recency-weighted percentiles that forget an old spike
 *   <li>value-tagging     - slice latency by venue at query time
 *   <li>iterators         - export the distribution as bands for a chart / sink
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws InterruptedException {
        baseTickToTrade();
        concurrentFeedHandlers();
        dualRecorderIntervalReport();
        mergeShardRollup();
        decayRecencyWeighted();
        valueTaggingByVenue();
        iteratorsExportBands();
    }

    /** Base API: record tick-to-trade latencies, read percentiles, then show the
     *  coordinated-omission correction lifting the tail. */
    static void baseTickToTrade() {
        System.out.println("== base: tick-to-trade latency capture ==");
        HdrHistogram h = new HdrHistogram(3);

        // Right-skewed latency stream via a deterministic xorshift.
        long rng = 0x2545F4914F6CDD1DL;
        long n = 2_000;
        for (long i = 0; i < n; i++) {
            rng ^= rng << 13;
            rng ^= rng >>> 7;
            rng ^= rng << 17;
            if (i % 50 == 0) {
                h.record(4_000 + Math.floorMod(rng, 4_000)); // ~2% tail spike, 4us..8us
            } else {
                h.record(700 + Math.floorMod(rng, 300));     // 700..1000 ns steady state
            }
        }

        long p50 = h.valueAtPercentile(0.50);
        long p99 = h.valueAtPercentile(0.99);
        long p999 = h.valueAtPercentile(0.999);
        System.out.println("  n=" + n + " p50=" + p50 + "ns p99=" + p99
            + "ns p999=" + p999 + "ns max=" + h.max() + "ns");
        if (h.count() != n) throw new AssertionError("every sample recorded");
        if (p50 > 1_100) throw new AssertionError("median in the steady band: " + p50);
        if (p99 < 2_000) throw new AssertionError("the tail lifts p99: " + p99);
        if (p999 < p99 || h.max() < p999) throw new AssertionError("monotone tail");

        // The reporting surface a dashboard actually wants alongside the
        // percentiles: the floor, the mean, the fraction inside the SLO, and
        // what the whole thing costs in memory.
        double withinSlo = h.percentileAtOrBelowValue(2_000);
        System.out.println(String.format("  min=%dns mean=%.0fns within-2us=%.1f%% footprint=%dKB",
            h.min(), h.mean(), withinSlo * 100.0, h.footprintBytes() / 1024));
        if (withinSlo <= 0.9) throw new AssertionError("most ops sit inside the 2us band");

        // Coordinated omission: a fixed-rate loop issues one op every 10 ns, then
        // stalls for 1000 ns. The naive histogram sees one slow sample; the
        // corrected one backfills the 99 requests the stall blocked.
        HdrHistogram naive = new HdrHistogram(3);
        HdrHistogram corrected = new HdrHistogram(3);
        for (int i = 0; i < 1_000; i++) {
            naive.record(10);
            corrected.recordWithExpectedInterval(10, 10);
        }
        naive.record(1_000);
        corrected.recordWithExpectedInterval(1_000, 10);
        long naiveP99 = naive.valueAtPercentile(0.99);
        long correctedP99 = corrected.valueAtPercentile(0.99);
        System.out.println("  coordinated omission: naive p99=" + naiveP99
            + "ns, corrected p99=" + correctedP99 + "ns");
        if (naiveP99 > 20) throw new AssertionError("uncorrected tail hides the stall: " + naiveP99);
        if (correctedP99 < 500) throw new AssertionError("correction lifts the tail: " + correctedP99);
    }

    /** concurrent-writes: several feed handlers record into one histogram
     *  from different threads with no external lock. */
    static void concurrentFeedHandlers() throws InterruptedException {
        System.out.println("\n== concurrent-writes: many feed handlers, one histogram ==");
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        int threads = 4;
        long perThread = 50_000;
        List<Thread> workers = new ArrayList<>();
        for (int t = 0; t < threads; t++) {
            Thread worker = new Thread(() -> {
                for (long i = 0; i < perThread; i++) h.record((i % 1_000) + 500);
            });
            workers.add(worker);
            worker.start();
        }
        for (Thread worker : workers) worker.join();
        System.out.println("  " + h.count() + " records lock-free, p99="
            + h.valueAtPercentile(0.99) + "ns");
        if (h.count() != threads * perThread) throw new AssertionError("no writes lost under contention");
    }

    /** dual-recorder: producers record continuously; a reporter drains an
     *  interval snapshot by rotating the active side, never blocking producers. */
    static void dualRecorderIntervalReport() {
        System.out.println("\n== dual-recorder: lock-free interval percentile report ==");
        DualRecorder rec = new DualRecorder(3);
        for (long v = 1; v <= 500; v++) rec.record(v);
        Snapshot interval = rec.getIntervalHistogram();
        System.out.println("  interval count=" + interval.count()
            + ", p99=" + interval.valueAtPercentile(0.99));
        Snapshot next = rec.getIntervalHistogram();
        if (interval.count() != 500) throw new AssertionError("first interval captured every record");
        if (next.count() != 0) throw new AssertionError("the next interval starts empty after the rotate");
    }

    /** merge: two shards each keep their own histogram; a roll-up sums one into
     *  the other for a fleet-wide percentile view. */
    static void mergeShardRollup() {
        System.out.println("\n== merge: roll per-shard histograms into a fleet view ==");
        HdrHistogram shardA = new HdrHistogram(3);
        HdrHistogram shardB = new HdrHistogram(3);
        for (long v = 1; v <= 500; v++) shardA.record(v);
        for (long v = 501; v <= 1_000; v++) shardB.record(v);
        Merge.merge(shardA, shardB);
        System.out.println("  fleet count=" + shardA.count()
            + ", p50=" + shardA.valueAtPercentile(0.5)
            + ", p99=" + shardA.valueAtPercentile(0.99));
        if (shardA.count() != 1_000) throw new AssertionError("both shards folded in");
        if (shardA.valueAtPercentile(0.99) < 900) throw new AssertionError("the high tail came from shard b");
    }

    /** decay: an old burst of slow ops fades over a few half-lives, so a later
     *  burst of fast ops dominates the read. */
    static void decayRecencyWeighted() {
        System.out.println("\n== decay: recency-weighted p50 forgets an old spike ==");
        ManualClock clock = new ManualClock();
        long halflife = 1_000_000_000L; // 1 second
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clock);
        for (int i = 0; i < 1_000; i++) h.record(5_000); // old burst of slow ops
        clock.advanceNs(halflife * 4);                   // four half-lives pass
        for (int i = 0; i < 1_000; i++) h.record(800);   // recent burst of fast ops
        long p50 = h.valueAtPercentile(0.5);
        System.out.println("  decayed count~" + Math.round(h.count()) + ", p50=" + p50 + "ns");
        if (p50 >= 2_000) throw new AssertionError("recent fast ops dominate: p50=" + p50);
    }

    /** value-tagging: one histogram, a 1-byte tag per recording, so per-venue
     *  tails can be read separately at query time. */
    static void valueTaggingByVenue() {
        System.out.println("\n== value-tagging: slice latency by venue ==");
        final byte colo = 0;
        final byte remote = 1;
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        for (long v = 500; v <= 1_000; v++) h.record(v, colo);   // fast co-located venue
        for (long v = 5_000; v <= 6_000; v++) h.record(v, remote); // slow remote venue
        long p99Colo = h.valueAtPercentileForTag(0.99, colo);
        long p99Remote = h.valueAtPercentileForTag(0.99, remote);
        System.out.println("  colo p99=" + p99Colo + "ns, remote p99=" + p99Remote + "ns");
        if (p99Colo >= p99Remote) throw new AssertionError("each venue's tail reads on its own");
    }

    /** iterators: walk the whole distribution - the powers-of-two bands and
     *  quartile lower bounds a chart or sink would render. */
    static void iteratorsExportBands() {
        System.out.println("\n== iterators: export the distribution as bands ==");
        HdrHistogram h = new HdrHistogram(3);
        for (long v = 1; v <= 1_000; v++) h.record(v);
        int bands = 0;
        for (Iterator<IterEntry> it = HdrIterators.logarithmic(h); it.hasNext(); it.next()) bands++;
        List<Long> quartiles = new ArrayList<>();
        for (Iterator<IterEntry> it = HdrIterators.percentiles(h, 25.0); it.hasNext(); ) {
            quartiles.add(it.next().valueLo);
        }
        System.out.println("  " + bands + " log2 bands; quartile lower bounds = " + quartiles);
        if (bands <= 0) throw new AssertionError("the populated range spans at least one band");
        if (quartiles.isEmpty()) throw new AssertionError("the percentile walk yields quartile buckets");
    }
}
