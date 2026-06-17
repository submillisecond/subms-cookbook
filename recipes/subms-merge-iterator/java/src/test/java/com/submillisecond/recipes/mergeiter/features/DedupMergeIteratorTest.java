package com.submillisecond.recipes.mergeiter.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class DedupMergeIteratorTest {

    private static DedupEntry<String, String> e(String k, String v) {
        return new DedupEntry<>(k, v);
    }

    @SafeVarargs
    private static List<Iterator<? extends DedupEntry<String, String>>> sources(
            List<DedupEntry<String, String>>... streams) {
        List<Iterator<? extends DedupEntry<String, String>>> out = new ArrayList<>();
        for (List<DedupEntry<String, String>> s : streams) out.add(s.iterator());
        return out;
    }

    private static List<DedupEntry<String, String>> drain(DedupMergeIterator<String, String> it) {
        List<DedupEntry<String, String>> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        return out;
    }

    @Test
    void distinctKeysPassThrough() {
        DedupMergeIterator<String, String> it = new DedupMergeIterator<>(sources(
                List.of(e("a", "1"), e("c", "3")),
                List.of(e("b", "2"), e("d", "4"))));
        assertEquals(List.of(e("a", "1"), e("b", "2"), e("c", "3"), e("d", "4")), drain(it));
    }

    @Test
    void duplicateKeyPicksLatestSource() {
        DedupMergeIterator<String, String> it = new DedupMergeIterator<>(sources(
                List.of(e("k", "old")),
                List.of(e("k", "new"))));
        assertEquals(List.of(e("k", "new")), drain(it));
    }

    @Test
    void threeWayDuplicatePicksHighestSource() {
        DedupMergeIterator<String, String> it = new DedupMergeIterator<>(sources(
                List.of(e("k", "v0")),
                List.of(e("k", "v1")),
                List.of(e("k", "v2"))));
        assertEquals(List.of(e("k", "v2")), drain(it));
    }

    @Test
    void emptySourcesYieldEmpty() {
        DedupMergeIterator<String, String> it = new DedupMergeIterator<>(sources(
                List.<DedupEntry<String, String>>of(),
                List.<DedupEntry<String, String>>of()));
        assertFalse(it.hasNext());
    }

    @Test
    void interleavedWithSomeDuplicates() {
        DedupMergeIterator<String, String> it = new DedupMergeIterator<>(sources(
                List.of(e("a", "a0"), e("b", "b0"), e("c", "c0")),
                List.of(e("b", "b1"), e("d", "d1"))));
        assertEquals(List.of(e("a", "a0"), e("b", "b1"), e("c", "c0"), e("d", "d1")), drain(it));
    }

    @Test
    void dedupPreservesCountWithUniqueKeys() {
        List<DedupEntry<Integer, Integer>> s0 = new ArrayList<>();
        List<DedupEntry<Integer, Integer>> s1 = new ArrayList<>();
        for (int i = 0; i < 100; i++) {
            s0.add(new DedupEntry<>(i * 2, i));
            s1.add(new DedupEntry<>(i * 2 + 1, i));
        }
        List<Iterator<? extends DedupEntry<Integer, Integer>>> srcs = new ArrayList<>();
        srcs.add(s0.iterator());
        srcs.add(s1.iterator());
        DedupMergeIterator<Integer, Integer> it = new DedupMergeIterator<>(srcs);
        List<DedupEntry<Integer, Integer>> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        assertEquals(200, out.size());
        for (int i = 1; i < out.size(); i++) {
            assertTrue(out.get(i - 1).key() < out.get(i).key());
        }
    }

    @Test
    void allDuplicatesCollapsesToOnePerKey() {
        List<DedupEntry<Integer, String>> a = new ArrayList<>();
        List<DedupEntry<Integer, String>> b = new ArrayList<>();
        List<DedupEntry<Integer, String>> c = new ArrayList<>();
        for (int k = 0; k < 5; k++) {
            a.add(new DedupEntry<>(k, "a-" + k));
            b.add(new DedupEntry<>(k, "b-" + k));
            c.add(new DedupEntry<>(k, "c-" + k));
        }
        List<Iterator<? extends DedupEntry<Integer, String>>> srcs = new ArrayList<>();
        srcs.add(a.iterator());
        srcs.add(b.iterator());
        srcs.add(c.iterator());
        DedupMergeIterator<Integer, String> it = new DedupMergeIterator<>(srcs);
        List<DedupEntry<Integer, String>> out = new ArrayList<>();
        while (it.hasNext()) out.add(it.next());
        // Latest source (c) wins for every key.
        List<DedupEntry<Integer, String>> expected = new ArrayList<>();
        for (int k = 0; k < 5; k++) expected.add(new DedupEntry<>(k, "c-" + k));
        assertEquals(expected, out);
    }
}
