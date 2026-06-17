package com.submillisecond.recipes.hdrhist.features;

import com.submillisecond.recipes.hdrhist.HdrHistogram;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

final class HdrIteratorsTest {

    private static List<IterEntry> collect(Iterator<IterEntry> it) {
        List<IterEntry> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        return out;
    }

    @Test
    void linearVisitsOnlyPopulatedBuckets() {
        HdrHistogram h = new HdrHistogram(3);
        for (long v : new long[]{10, 100, 1000}) h.record(v);
        List<IterEntry> entries = collect(HdrIterators.linear(h));
        assertEquals(3, entries.size(), "exactly the three populated buckets");
        for (int i = 1; i < entries.size(); i++) {
            assertTrue(entries.get(i - 1).valueLo < entries.get(i).valueLo, "value order");
        }
        assertEquals(3, entries.get(entries.size() - 1).cumulative);
    }

    @Test
    void linearOnEmptyYieldsNothing() {
        HdrHistogram h = new HdrHistogram(3);
        assertFalse(HdrIterators.linear(h).hasNext());
    }

    @Test
    void logarithmicBandsDouble() {
        HdrHistogram h = new HdrHistogram(3);
        for (long v : new long[]{1, 3, 7, 15, 31, 100, 1000}) h.record(v);
        List<IterEntry> entries = collect(HdrIterators.logarithmic(h));
        assertFalse(entries.isEmpty());
        for (int i = 1; i < entries.size(); i++) {
            assertEquals(entries.get(i - 1).valueHi, entries.get(i).valueLo, "abutting bands");
            assertEquals(entries.get(i - 1).valueLo * 2, entries.get(i - 1).valueHi, "powers of two");
        }
        long total = entries.stream().mapToLong(e -> e.count).sum();
        assertEquals(7, total);
    }

    @Test
    void logarithmicCoversHighBucket() {
        HdrHistogram h = new HdrHistogram(3);
        h.record(1);
        h.record(1_000_000);
        List<IterEntry> entries = collect(HdrIterators.logarithmic(h));
        long total = entries.stream().mapToLong(e -> e.count).sum();
        assertEquals(2, total);
    }

    @Test
    void percentileEmitsRoughlyStepEntries() {
        HdrHistogram h = new HdrHistogram(3);
        for (long v = 1; v <= 1000; v++) h.record(v);
        List<IterEntry> entries = collect(HdrIterators.percentiles(h, 10.0));
        assertTrue(entries.size() >= 8 && entries.size() <= 12,
                "got " + entries.size() + " entries");
        for (int i = 1; i < entries.size(); i++) {
            assertTrue(entries.get(i - 1).cumulative <= entries.get(i).cumulative);
        }
    }

    @Test
    void percentileOnEmptyYieldsNothing() {
        HdrHistogram h = new HdrHistogram(3);
        assertFalse(HdrIterators.percentiles(h, 1.0).hasNext());
    }

    @Test
    void linearTotalEqualsRecorded() {
        HdrHistogram h = new HdrHistogram(3);
        for (long v = 1; v <= 100; v++) h.record(v);
        long linearTotal = 0;
        Iterator<IterEntry> it = HdrIterators.linear(h);
        while (it.hasNext()) linearTotal += it.next().count;
        assertEquals(100, linearTotal);
    }
}
