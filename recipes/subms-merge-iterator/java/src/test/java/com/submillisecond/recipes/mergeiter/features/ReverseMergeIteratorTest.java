package com.submillisecond.recipes.mergeiter.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class ReverseMergeIteratorTest {

    /** Descending sources - the direction this iterator contracts for. */
    @SafeVarargs
    private static ReverseMergeIterator<Integer> merge(List<Integer>... data) {
        List<Iterator<Integer>> streams = new ArrayList<>();
        for (List<Integer> d : data) streams.add(d.iterator());
        return new ReverseMergeIterator<>(streams);
    }

    private static List<Integer> drain(ReverseMergeIterator<Integer> it) {
        List<Integer> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        return out;
    }

    @Test
    void mergesDescendingStreams() {
        ReverseMergeIterator<Integer> it =
            merge(List.of(7, 4, 1), List.of(8, 5, 2), List.of(9, 6, 3));
        assertEquals(List.of(9, 8, 7, 6, 5, 4, 3, 2, 1), drain(it));
    }

    @Test
    void freshIteratorSitsOnTheLargestValue() {
        ReverseMergeIterator<Integer> it = merge(List.of(40, 10), List.of(90, 20));
        assertEquals(90, it.peek());
        assertEquals(2, it.liveStreams());
        assertEquals(2, it.numStreams());
    }

    @Test
    void handlesEmptyAndAbsentStreams() {
        ReverseMergeIterator<Integer> it = merge(List.of(), List.of(5, 1), List.of());
        assertEquals(3, it.numStreams());
        assertEquals(1, it.liveStreams());
        assertEquals(List.of(5, 1), drain(it));

        ReverseMergeIterator<Integer> empty = new ReverseMergeIterator<>(List.of());
        assertNull(empty.peek());
        assertFalse(empty.hasNext());
        assertThrows(NoSuchElementException.class, empty::next);
    }

    @Test
    void duplicatesAcrossStreamsAllAppear() {
        ReverseMergeIterator<Integer> it = merge(List.of(3, 2, 1), List.of(4, 3, 2));
        assertEquals(List.of(4, 3, 3, 2, 2, 1), drain(it));
    }

    @Test
    void seekForPrevLandsOnLargestLeTarget() {
        ReverseMergeIterator<Integer> it =
            merge(List.of(10, 7, 4, 1), List.of(11, 8, 5, 2), List.of(12, 9, 6, 3));
        it.seekForPrev(6);
        assertEquals(List.of(6, 5, 4, 3, 2, 1), drain(it));
    }

    @Test
    void seekForPrevAboveEveryValueIsNoop() {
        ReverseMergeIterator<Integer> it = merge(List.of(7, 6, 5), List.of(10, 9, 8));
        it.seekForPrev(100);
        assertEquals(List.of(10, 9, 8, 7, 6, 5), drain(it));
    }

    @Test
    void seekForPrevBelowEveryValueExhausts() {
        ReverseMergeIterator<Integer> it = merge(List.of(3, 2, 1), List.of(6, 5, 4));
        it.seekForPrev(0);
        assertFalse(it.hasNext());
        assertThrows(NoSuchElementException.class, it::next);
    }

    @Test
    void seekForPrevOnEmptySourcesIsSafe() {
        ReverseMergeIterator<Integer> it = merge(List.of(), List.of(), List.of());
        it.seekForPrev(42);
        assertFalse(it.hasNext());
    }

    @Test
    void repeatedSeekForPrevIsMonotonic() {
        ReverseMergeIterator<Integer> it =
            merge(List.of(10, 9, 8, 7, 6, 5, 4, 3, 2, 1), List.of(17, 16, 15));
        it.seekForPrev(16);
        assertEquals(16, it.next());
        it.seekForPrev(9);
        assertEquals(9, it.next());
        it.seekForPrev(6);
        assertEquals(6, it.next());
        it.seekForPrev(0);
        assertFalse(it.hasNext());
    }

    @Test
    void seekForPrevOnlyWalksStreamsAboveTarget() {
        // One head already <= target takes the early-stop arm; the other two
        // walk forward. Both arms run inside one call.
        ReverseMergeIterator<Integer> it =
            merge(List.of(50, 2, 1), List.of(31, 30), List.of(40, 5));
        it.seekForPrev(31);
        assertEquals(List.of(31, 30, 5, 2, 1), drain(it));
    }

    @Test
    void lowerBoundIsInclusiveAndStopsTheScan() {
        ReverseMergeIterator<Integer> it = merge(List.of(9, 6, 3), List.of(8, 5, 2));
        it.setLowerBound(5);
        assertEquals(List.of(9, 8, 6, 5), drain(it));
    }

    @Test
    void lowerBoundCanBeClearedAndTheRestStillReads() {
        ReverseMergeIterator<Integer> it = merge(List.of(9, 6, 3), List.of(8, 5, 2));
        it.setLowerBound(6);
        assertEquals(List.of(9, 8, 6), drain(it));
        assertNull(it.peek());
        it.clearLowerBound();
        assertEquals(List.of(5, 3, 2), drain(it));
    }

    @Test
    void lowerBoundAboveTheHeadExhaustsImmediately() {
        ReverseMergeIterator<Integer> it = merge(List.of(4, 3), List.of(2, 1));
        it.setLowerBound(99);
        assertNull(it.peek());
        assertFalse(it.hasNext());
    }

    @Test
    void boundedWindowScanReadsAPriceBand() {
        // seekForPrev(hi) + setLowerBound(lo) is the descending [lo, hi]
        // window, the shape a book walk from the top of book downward wants.
        ReverseMergeIterator<Integer> it =
            merge(List.of(120, 105, 101, 95), List.of(118, 110, 99));
        it.seekForPrev(110);
        it.setLowerBound(100);
        assertEquals(List.of(110, 105, 101), drain(it));
    }

    @Test
    void descendingMergeHoldsOverTenStreams() {
        List<Iterator<Integer>> streams = new ArrayList<>();
        for (int s = 0; s < 10; s++) {
            Integer[] vals = new Integer[1000];
            for (int i = 0; i < 1000; i++) vals[i] = s + 10 * (999 - i);
            streams.add(Arrays.asList(vals).iterator());
        }
        ReverseMergeIterator<Integer> it = new ReverseMergeIterator<>(streams);
        List<Integer> merged = drain(it);
        assertEquals(10_000, merged.size());
        for (int i = 1; i < merged.size(); i++) {
            assertFalse(merged.get(i - 1) < merged.get(i), "descending");
        }
    }
}
