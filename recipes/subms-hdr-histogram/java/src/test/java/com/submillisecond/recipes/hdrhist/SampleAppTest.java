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
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        HdrHistogram h = new HdrHistogram(3);              // 3 significant digits
        for (long v : new long[] {10, 20, 30, 40, 50}) h.record(v);
        assertEquals(5, h.count());
        long p50 = h.valueAtPercentile(0.5);               // a value in [20, 30]
        assertTrue(p50 >= 20 && p50 <= 30, "p50=" + p50);
        assertEquals(50, h.max());
        // quickstart:end
    }

    @Test
    void tickToTradePercentilesAreMonotone() {
        HdrHistogram h = new HdrHistogram(3);
        long rng = 0x2545F4914F6CDD1DL;
        long n = 2_000;
        for (long i = 0; i < n; i++) {
            rng ^= rng << 13;
            rng ^= rng >>> 7;
            rng ^= rng << 17;
            if (i % 50 == 0) h.record(4_000 + Math.floorMod(rng, 4_000));
            else h.record(700 + Math.floorMod(rng, 300));
        }
        assertEquals(n, h.count());
        long p50 = h.valueAtPercentile(0.50);
        long p99 = h.valueAtPercentile(0.99);
        long p999 = h.valueAtPercentile(0.999);
        assertTrue(p50 <= 1_100, "median in the steady band: " + p50);
        assertTrue(p99 >= 2_000, "the tail lifts p99: " + p99);
        assertTrue(p999 >= p99 && h.max() >= p999, "monotone tail");

        assertTrue(h.min() > 0, "the floor is a real recorded value");
        assertTrue(h.mean() >= (double) p50, "the tail drags the mean above the median");
        assertTrue(h.percentileAtOrBelowValue(2_000) > 0.9, "most ops sit inside the 2us band");
        assertEquals(0, h.footprintBytes() % 8, "the array is long counters");
    }

    @Test
    void coordinatedOmissionLiftsTheTail() {
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
        assertTrue(naiveP99 <= 20, "uncorrected tail hides the stall: " + naiveP99);
        assertTrue(correctedP99 >= 500, "correction backfills blocked requests: " + correctedP99);
        assertTrue(correctedP99 > naiveP99, "corrected tail is strictly higher");
    }

    @Test
    void concurrentWritersLoseNothing() throws InterruptedException {
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
        assertEquals(threads * perThread, h.count());
    }

    @Test
    void dualRecorderIntervalThenEmpty() {
        DualRecorder rec = new DualRecorder(3);
        for (long v = 1; v <= 500; v++) rec.record(v);
        Snapshot interval = rec.getIntervalHistogram();
        Snapshot next = rec.getIntervalHistogram();
        assertEquals(500, interval.count());
        assertEquals(0, next.count());
    }

    @Test
    void mergeRollsShardsIntoFleet() {
        HdrHistogram shardA = new HdrHistogram(3);
        HdrHistogram shardB = new HdrHistogram(3);
        for (long v = 1; v <= 500; v++) shardA.record(v);
        for (long v = 501; v <= 1_000; v++) shardB.record(v);
        Merge.merge(shardA, shardB);
        assertEquals(1_000, shardA.count());
        assertTrue(shardA.valueAtPercentile(0.99) >= 900);
    }

    @Test
    void decayWeightsRecentActivity() {
        ManualClock clock = new ManualClock();
        long halflife = 1_000_000_000L;
        DecayingHdrHistogram h = new DecayingHdrHistogram(3, halflife, clock);
        for (int i = 0; i < 1_000; i++) h.record(5_000);
        clock.advanceNs(halflife * 4);
        for (int i = 0; i < 1_000; i++) h.record(800);
        assertTrue(h.valueAtPercentile(0.5) < 2_000, "recent fast ops dominate");
    }

    @Test
    void valueTaggingSeparatesVenues() {
        final byte colo = 0;
        final byte remote = 1;
        TaggedHdrHistogram h = new TaggedHdrHistogram(3);
        for (long v = 500; v <= 1_000; v++) h.record(v, colo);
        for (long v = 5_000; v <= 6_000; v++) h.record(v, remote);
        assertTrue(h.valueAtPercentileForTag(0.99, colo) < h.valueAtPercentileForTag(0.99, remote));
    }

    @Test
    void iteratorsExportBandsAndQuartiles() {
        HdrHistogram h = new HdrHistogram(3);
        for (long v = 1; v <= 1_000; v++) h.record(v);
        int bands = 0;
        for (Iterator<IterEntry> it = HdrIterators.logarithmic(h); it.hasNext(); it.next()) bands++;
        assertTrue(bands > 0);
        List<Long> quartiles = new ArrayList<>();
        for (Iterator<IterEntry> it = HdrIterators.percentiles(h, 25.0); it.hasNext(); ) {
            quartiles.add(it.next().valueLo);
        }
        assertFalse(quartiles.isEmpty());
    }
}
