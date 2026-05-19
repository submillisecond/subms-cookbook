package com.submillisecond.recipes.mergeiter;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MergeIteratorTest {

    private static List<Integer> merge(List<List<Integer>> streams) {
        List<Iterator<Integer>> iters = new ArrayList<>();
        for (List<Integer> s : streams) iters.add(s.iterator());
        MergeIterator<Integer> m = new MergeIterator<>(iters);
        List<Integer> out = new ArrayList<>();
        while (m.hasNext()) out.add(m.next());
        return out;
    }

    @Test
    void mergesThreeStreams() {
        List<Integer> out = merge(List.of(
            List.of(1, 4, 7),
            List.of(2, 5, 8),
            List.of(3, 6, 9)));
        assertEquals(List.of(1, 2, 3, 4, 5, 6, 7, 8, 9), out);
    }

    @Test
    void handlesEmptyStreams() {
        assertEquals(List.of(5, 10), merge(List.of(List.of(), List.of(5, 10), List.of())));
    }

    @Test
    void handlesDuplicates() {
        assertEquals(Arrays.asList(1, 2, 2, 3, 3, 4),
            merge(List.of(List.of(1, 2, 3), List.of(2, 3, 4))));
    }

    @Test
    void singleStreamPassesThrough() {
        assertEquals(List.of(1, 2, 3), merge(List.of(List.of(1, 2, 3))));
    }

    @Test
    void noStreamsYieldsEmpty() {
        assertTrue(merge(List.of()).isEmpty());
    }

    @Test
    void nextThrowsWhenExhausted() {
        MergeIterator<Integer> m = new MergeIterator<>(List.of(List.<Integer>of().iterator()));
        assertFalse(m.hasNext());
        assertThrows(NoSuchElementException.class, m::next);
    }

    @Test
    void oneLongOneShortStream() {
        List<Integer> out = merge(List.of(
            Arrays.asList(1, 2, 3, 4, 5, 6, 7, 8, 9, 10),
            List.of(5)));
        assertEquals(Arrays.asList(1, 2, 3, 4, 5, 5, 6, 7, 8, 9, 10), out);
    }

    @Test
    void streamsWithNegativeValues() {
        List<Integer> out = merge(List.of(
            Arrays.asList(-5, -1, 0),
            Arrays.asList(-3, 2, 4)));
        assertEquals(Arrays.asList(-5, -3, -1, 0, 2, 4), out);
    }

    @Test
    void mergePreservesTotalCount() {
        int nStreams = 5;
        int per = 200;
        List<List<Integer>> data = new ArrayList<>();
        for (int s = 0; s < nStreams; s++) {
            List<Integer> stream = new ArrayList<>();
            for (int i = 0; i < per; i++) stream.add(s * 1000 + i);
            data.add(stream);
        }
        assertEquals(nStreams * per, merge(data).size());
    }
}
