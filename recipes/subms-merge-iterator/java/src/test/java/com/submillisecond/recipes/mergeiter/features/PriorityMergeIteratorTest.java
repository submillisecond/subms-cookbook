package com.submillisecond.recipes.mergeiter.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;

final class PriorityMergeIteratorTest {

    private static PriorityEntry<String, String> e(String k, String v) {
        return new PriorityEntry<>(k, v);
    }

    private static PrioritySource<String, String> src(
            int prio, List<PriorityEntry<String, String>> entries) {
        return new PrioritySource<>(prio, entries.iterator());
    }

    private static List<PriorityEntry<String, String>> drain(
            PriorityMergeIterator<String, String> it) {
        List<PriorityEntry<String, String>> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        return out;
    }

    @Test
    void higherPriorityWinsOnTie() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of(
                src(10, List.of(e("k", "high"))),
                src(1, List.of(e("k", "low")))));
        assertEquals(List.of(e("k", "high")), drain(it));
    }

    @Test
    void equalPriorityFallsBackToRegistrationOrder() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of(
                src(5, List.of(e("k", "first"))),
                src(5, List.of(e("k", "second")))));
        assertEquals(List.of(e("k", "second")), drain(it));
    }

    @Test
    void threeSourcesPriorityTieBreak() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of(
                src(1, List.of(e("k", "p1"))),
                src(100, List.of(e("k", "p100"))),
                src(50, List.of(e("k", "p50")))));
        assertEquals(List.of(e("k", "p100")), drain(it));
    }

    @Test
    void distinctKeysYieldEveryEntry() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of(
                src(10, List.of(e("a", "a-hi"), e("c", "c-hi"))),
                src(1, List.of(e("b", "b-lo"), e("d", "d-lo")))));
        assertEquals(List.of(e("a", "a-hi"), e("b", "b-lo"), e("c", "c-hi"), e("d", "d-lo")), drain(it));
    }

    @Test
    void emptySourcesYieldEmpty() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of());
        assertFalse(it.hasNext());
    }

    @Test
    void negativePriorityLosesToZero() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of(
                src(0, List.of(e("k", "zero"))),
                src(-100, List.of(e("k", "neg")))));
        assertEquals(List.of(e("k", "zero")), drain(it));
    }

    @Test
    void mixedKeysAndPriorities() {
        PriorityMergeIterator<String, String> it = new PriorityMergeIterator<>(List.of(
                src(10, List.of(e("a", "a-hi"), e("c", "c-hi"), e("e", "e-hi"))),
                src(1, List.of(e("a", "a-lo"), e("b", "b-lo"), e("c", "c-lo")))));
        assertEquals(
                List.of(e("a", "a-hi"), e("b", "b-lo"), e("c", "c-hi"), e("e", "e-hi")),
                drain(it));
    }
}
