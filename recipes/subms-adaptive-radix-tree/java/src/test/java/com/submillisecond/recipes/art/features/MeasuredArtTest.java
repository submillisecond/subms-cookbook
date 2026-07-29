package com.submillisecond.recipes.art.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MeasuredArtTest {

    @Test
    void emptyMetricsZeroEverywhere() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        ArtMetrics snap = m.metrics();
        assertEquals(0, snap.lookups);
        assertEquals(0, snap.insertions);
        assertEquals(0, snap.deletions);
        assertEquals(0, snap.lastDepth);
        assertEquals(0, snap.entries);
        // Root node exists as a Node4 at construction.
        assertEquals(1, snap.node4);
        assertEquals(0, snap.node16);
    }

    @Test
    void countersTrackOperations() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        m.insert("alice".getBytes(), 1);
        m.insert("bob".getBytes(), 2);
        m.insert("alex".getBytes(), 3);
        m.get("alice".getBytes());
        m.get("missing".getBytes());
        m.delete("alex".getBytes());
        ArtMetrics snap = m.metrics();
        assertEquals(3, snap.insertions);
        assertEquals(2, snap.lookups);
        assertEquals(1, snap.deletions);
        assertEquals(2, snap.entries);
    }

    @Test
    void lastDepthReflectsKeyLength() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        m.insert("abcdefghij".getBytes(), 1);
        assertEquals(10, m.metrics().lastDepth);
        m.get("x".getBytes());
        assertEquals(1, m.metrics().lastDepth);
        m.delete("abcdefghij".getBytes());
        assertEquals(10, m.metrics().lastDepth);
    }

    @Test
    void nodeTypeDistributionChangesWithGrowth() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        // 4 distinct first bytes -> root Node4 + 4 leaves (each an empty Node4).
        for (int i = 0; i < 4; i++) {
            m.insert(new byte[]{(byte) i}, i);
        }
        ArtMetrics before = m.metrics();
        assertEquals(0, before.node16, "no Node16 yet: node16=" + before.node16);
        assertEquals(5, before.node4, "root + 4 leaves: node4=" + before.node4);
        // 5th distinct first byte promotes the root Node4 -> Node16.
        m.insert(new byte[]{(byte) 4}, 4);
        ArtMetrics after = m.metrics();
        assertEquals(1, after.node16, "root now Node16: node16=" + after.node16);
        assertEquals(5, after.node4, "5 leaves remain Node4: node4=" + after.node4);
    }

    @Test
    void counterOverflowUsesSaturatingArithmetic() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        m.setLookupsForTest(Long.MAX_VALUE - 1);
        m.get("x".getBytes());
        assertEquals(Long.MAX_VALUE, m.metrics().lookups);
        m.get("y".getBytes());
        assertEquals(Long.MAX_VALUE, m.metrics().lookups, "saturating not wrapping");
    }

    @Test
    void metricsSnapshotIsIndependentOfSubsequentOps() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        m.insert("a".getBytes(), 1);
        ArtMetrics snap = m.metrics();
        m.insert("b".getBytes(), 2);
        m.insert("c".getBytes(), 3);
        assertEquals(1, snap.insertions);
        assertEquals(1, snap.entries);
        assertEquals(3, m.metrics().insertions);
    }

    @Test
    void treeAccessorComposesWithBaseApi() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        m.insert("alpha".getBytes(), 1);
        m.insert("beta".getBytes(), 2);
        assertEquals(1, m.tree().get("alpha".getBytes()));
        assertEquals(2, m.tree().get("beta".getBytes()));
        // Inner lookups via tree() don't bump lookups counter.
        assertEquals(0, m.metrics().lookups);
    }

    @Test
    void deletedKeyNoLongerVisible() {
        MeasuredArt<Integer> m = new MeasuredArt<>();
        m.insert("present".getBytes(), 1);
        assertEquals(1, m.delete("present".getBytes()));
        assertNull(m.get("present".getBytes()));
        assertTrue(m.isEmpty());
    }
}
