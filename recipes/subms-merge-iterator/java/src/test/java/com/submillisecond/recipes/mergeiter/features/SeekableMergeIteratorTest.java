package com.submillisecond.recipes.mergeiter.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SeekableMergeIteratorTest {

    private static List<Iterator<Integer>> iters(List<List<Integer>> data) {
        List<Iterator<Integer>> out = new ArrayList<>();
        for (List<Integer> s : data) out.add(s.iterator());
        return out;
    }

    private static List<Integer> drain(SeekableMergeIterator<Integer> it) {
        List<Integer> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        return out;
    }

    @Test
    void seekLandsOnSmallestGeTarget() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(1, 4, 7, 10),
                List.of(2, 5, 8, 11),
                List.of(3, 6, 9, 12))));
        it.seek(6);
        assertEquals(List.of(6, 7, 8, 9, 10, 11, 12), drain(it));
    }

    @Test
    void seekWithTargetBelowMinIsNoop() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(5, 6, 7),
                List.of(8, 9, 10))));
        it.seek(0);
        assertEquals(List.of(5, 6, 7, 8, 9, 10), drain(it));
    }

    @Test
    void seekPastEndExhaustsIterator() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(1, 2, 3),
                List.of(4, 5, 6))));
        it.seek(1000);
        assertFalse(it.hasNext());
        assertThrows(NoSuchElementException.class, it::next);
    }

    @Test
    void seekOnEmptySourcesIsSafe() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(), List.of(), List.of())));
        it.seek(42);
        assertFalse(it.hasNext());
    }

    @Test
    void repeatedSeeksAreMonotonic() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(1, 2, 3, 4, 5, 6, 7, 8, 9, 10),
                List.of(15, 16, 17))));
        it.seek(5);
        assertEquals(Integer.valueOf(5), it.next());
        it.seek(8);
        assertEquals(Integer.valueOf(8), it.next());
        it.seek(14);
        assertEquals(Integer.valueOf(15), it.next());
        it.seek(20);
        assertFalse(it.hasNext());
    }

    @Test
    void seekTargetExactlyPresentYieldsIt() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(1, 5, 9),
                List.of(2, 6, 10),
                List.of(3, 7, 11))));
        it.seek(7);
        assertEquals(List.of(7, 9, 10, 11), drain(it));
    }

    @Test
    void seekWithSomeExhaustedStreams() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(1, 2, 3),
                List.of(10, 20, 30))));
        it.seek(15);
        assertEquals(List.of(20, 30), drain(it));
    }

    @Test
    void hasNextStaysAccurateAfterSeek() {
        SeekableMergeIterator<Integer> it = new SeekableMergeIterator<>(iters(List.of(
                List.of(1, 2),
                List.of(3, 4))));
        it.seek(3);
        assertTrue(it.hasNext());
        assertEquals(Integer.valueOf(3), it.next());
        assertTrue(it.hasNext());
        assertEquals(Integer.valueOf(4), it.next());
        assertFalse(it.hasNext());
    }
}
