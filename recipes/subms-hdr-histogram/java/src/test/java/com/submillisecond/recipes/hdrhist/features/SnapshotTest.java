package com.submillisecond.recipes.hdrhist.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SnapshotTest {

    @Test
    void drainingAnEmptyHistogramGivesAnEmptySnapshot() {
        Snapshot s = new ConcurrentHdrHistogram(3).drainSnapshot();
        assertEquals(0, s.count());
        assertEquals(0, s.max());
        assertEquals(0, s.valueAtPercentile(0.99));
    }

    @Test
    void snapshotPercentilesMatchTheDrainedDistribution() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        for (long i = 1; i <= 1000; i++) h.record(i);
        Snapshot s = h.drainSnapshot();

        assertEquals(1000, s.count());
        assertEquals(1000, s.max());
        long p50 = s.valueAtPercentile(0.50);
        long p99 = s.valueAtPercentile(0.99);
        assertTrue(p50 >= 450 && p50 <= 550, "p50=" + p50);
        assertTrue(p99 >= 950 && p99 <= 1050, "p99=" + p99);
    }

    @Test
    void quantileIsClampedAtBothEnds() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        for (long i = 1; i <= 100; i++) h.record(i);
        Snapshot s = h.drainSnapshot();
        assertEquals(1, s.valueAtPercentile(-1.0), "below 0 reads the minimum");
        assertEquals(100, s.valueAtPercentile(2.0), "above 1 reads the maximum");
    }

    @Test
    void largeValuesRoundTripThroughTheBucketInverse() {
        ConcurrentHdrHistogram h = new ConcurrentHdrHistogram(3);
        h.record(9_000_000L);
        Snapshot s = h.drainSnapshot();
        long max = s.max();
        assertTrue(max >= 8_950_000L && max <= 9_000_000L,
                "the bucket lower bound sits inside the error band: " + max);
    }
}
