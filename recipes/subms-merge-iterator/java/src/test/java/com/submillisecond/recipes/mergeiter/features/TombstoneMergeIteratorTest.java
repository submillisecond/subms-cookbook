package com.submillisecond.recipes.mergeiter.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TombstoneMergeIteratorTest {

    private static TombstoneEntry<String, String> live(String k, String v) {
        return TombstoneEntry.live(k, v);
    }
    private static TombstoneEntry<String, String> tomb(String k) {
        return TombstoneEntry.tombstone(k);
    }

    @SafeVarargs
    private static List<Iterator<? extends TombstoneEntry<String, String>>> sources(
            List<TombstoneEntry<String, String>>... streams) {
        List<Iterator<? extends TombstoneEntry<String, String>>> out = new ArrayList<>();
        for (List<TombstoneEntry<String, String>> s : streams) out.add(s.iterator());
        return out;
    }

    private static List<TombstoneEntry<String, String>> drain(
            TombstoneMergeIterator<String, String> it) {
        List<TombstoneEntry<String, String>> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        return out;
    }

    @Test
    void liveEntriesPassThroughWhenNoTombstones() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(live("a", "1"), live("c", "3")),
                List.of(live("b", "2"), live("d", "4"))));
        assertEquals(List.of(live("a", "1"), live("b", "2"), live("c", "3"), live("d", "4")), drain(it));
    }

    @Test
    void laterSourceTombstoneHidesEarlierValue() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(live("k", "v")),
                List.of(tomb("k"))));
        assertFalse(it.hasNext());
    }

    @Test
    void laterSourceLiveOverwritesEarlierValue() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(live("k", "old")),
                List.of(live("k", "new"))));
        assertEquals(List.of(live("k", "new")), drain(it));
    }

    @Test
    void earlierSourceTombstoneDoesNotHideLaterValue() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(tomb("k")),
                List.of(live("k", "v"))));
        assertEquals(List.of(live("k", "v")), drain(it));
    }

    @Test
    void tombstoneShadowingSpansThreeSources() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(live("a", "1"), live("b", "2")),
                List.of(tomb("a")),
                List.of(live("c", "3"))));
        assertEquals(List.of(live("b", "2"), live("c", "3")), drain(it));
    }

    @Test
    void allSourcesEmptyYieldsNothing() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.<TombstoneEntry<String, String>>of(),
                List.<TombstoneEntry<String, String>>of()));
        assertFalse(it.hasNext());
    }

    @Test
    void allTombstonesYieldsNothing() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(tomb("a"), tomb("b")),
                List.of(tomb("a"), tomb("c"))));
        assertFalse(it.hasNext());
    }

    @Test
    void interleavedTombstonesAndLiveResolvePerKey() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(live("a", "1"), live("b", "2"), live("c", "3")),
                List.of(tomb("a"), live("b", "new")),
                List.of(tomb("b"))));
        assertEquals(List.of(live("c", "3")), drain(it));
    }

    @Test
    void singleSourceLivePassesThrough() {
        TombstoneMergeIterator<String, String> it = new TombstoneMergeIterator<>(sources(
                List.of(live("a", "1"), live("b", "2"))));
        assertTrue(it.hasNext());
        assertEquals(live("a", "1"), it.next());
        assertEquals(live("b", "2"), it.next());
        assertFalse(it.hasNext());
    }
}
