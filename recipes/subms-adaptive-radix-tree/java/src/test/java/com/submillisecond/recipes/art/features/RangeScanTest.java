package com.submillisecond.recipes.art.features;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.art.Art;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class RangeScanTest {

    private static Art<Integer> build(String... keys) {
        Art<Integer> t = new Art<>();
        for (int i = 0; i < keys.length; i++) {
            t.insert(keys[i].getBytes(), i);
        }
        return t;
    }

    private static List<String> keys(List<RangeScan.Entry<Integer>> r) {
        List<String> out = new ArrayList<>();
        for (RangeScan.Entry<Integer> e : r) out.add(new String(e.key));
        return out;
    }

    @Test
    void emptyTreeYieldsNothing() {
        Art<Integer> t = new Art<>();
        List<RangeScan.Entry<Integer>> out = RangeScan.range(t, RangeScan.Bound.unbounded(), RangeScan.Bound.unbounded());
        assertTrue(out.isEmpty());
    }

    @Test
    void unboundedScanReturnsAllKeysSorted() {
        Art<Integer> t = build("banana", "apple", "cherry", "avocado");
        List<RangeScan.Entry<Integer>> out = RangeScan.range(t, RangeScan.Bound.unbounded(), RangeScan.Bound.unbounded());
        assertEquals(List.of("apple", "avocado", "banana", "cherry"), keys(out));
    }

    @Test
    void inclusiveBoundsBothEndpointsMatch() {
        Art<Integer> t = build("a", "b", "c", "d", "e");
        List<RangeScan.Entry<Integer>> out = RangeScan.range(
            t, RangeScan.Bound.included("b".getBytes()), RangeScan.Bound.included("d".getBytes()));
        assertEquals(List.of("b", "c", "d"), keys(out));
    }

    @Test
    void exclusiveBoundsDropEndpoints() {
        Art<Integer> t = build("a", "b", "c", "d", "e");
        List<RangeScan.Entry<Integer>> out = RangeScan.range(
            t, RangeScan.Bound.excluded("b".getBytes()), RangeScan.Bound.excluded("d".getBytes()));
        assertEquals(List.of("c"), keys(out));
    }

    @Test
    void mixedBounds() {
        Art<Integer> t = build("a", "b", "c", "d", "e");
        List<RangeScan.Entry<Integer>> out = RangeScan.range(
            t, RangeScan.Bound.included("b".getBytes()), RangeScan.Bound.excluded("d".getBytes()));
        assertEquals(List.of("b", "c"), keys(out));
    }

    @Test
    void unboundedFromReturnsPrefix() {
        Art<Integer> t = build("a", "b", "c", "d");
        List<RangeScan.Entry<Integer>> out = RangeScan.range(
            t, RangeScan.Bound.unbounded(), RangeScan.Bound.included("b".getBytes()));
        assertEquals(List.of("a", "b"), keys(out));
    }

    @Test
    void emptyKeyIsMinimum() {
        Art<Integer> t = build("a", "b");
        t.insert(new byte[0], 99);
        List<RangeScan.Entry<Integer>> out = RangeScan.range(t, RangeScan.Bound.unbounded(), RangeScan.Bound.unbounded());
        assertEquals(3, out.size());
        assertEquals(0, out.get(0).key.length);
        assertEquals("a", new String(out.get(1).key));
        assertEquals("b", new String(out.get(2).key));
    }

    @Test
    void deepNodeKeysReturnedInOrder() {
        // Force root to grow Small -> Full.
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 256; i++) {
            byte[] key = new byte[]{(byte) i, 0, (byte) i};
            t.insert(key, i);
        }
        List<RangeScan.Entry<Integer>> out = RangeScan.range(
            t, RangeScan.Bound.included(new byte[]{10}), RangeScan.Bound.excluded(new byte[]{15}));
        List<Integer> starts = new ArrayList<>();
        for (RangeScan.Entry<Integer> e : out) starts.add(e.key[0] & 0xff);
        assertEquals(List.of(10, 11, 12, 13, 14), starts);
    }

    @Test
    void outOfRangeBoundsYieldEmpty() {
        Art<Integer> t = build("a", "b", "c");
        List<RangeScan.Entry<Integer>> out = RangeScan.range(
            t, RangeScan.Bound.included("x".getBytes()), RangeScan.Bound.included("z".getBytes()));
        assertTrue(out.isEmpty());
    }
}
