package com.submillisecond.recipes.cms.features;

import com.submillisecond.recipes.cms.CountMinSketch;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MergeTest {

    @Test
    void mergeDisjointKeysPreservesBothCounts() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 4096);
        for (int i = 0; i < 200; i++) a.add("alpha");
        for (int i = 0; i < 150; i++) b.add("beta");
        Merge.mergeInto(a, b);
        assertTrue(a.estimate("alpha") >= 200);
        assertTrue(a.estimate("beta") >= 150);
    }

    @Test
    void mergeSharedKeyKeepsTheOneSidedGuarantee() {
        // The key was seen 100 times on one shard and 300 on the other, so the
        // union count is 400. Summing cells is what keeps the estimate above
        // it; taking the max would report ~300 and break the guarantee.
        CountMinSketch a = new CountMinSketch(5, 16384);
        CountMinSketch b = new CountMinSketch(5, 16384);
        for (int i = 0; i < 100; i++) a.add("shared");
        for (int i = 0; i < 300; i++) b.add("shared");
        Merge.mergeInto(a, b);
        int est = a.estimate("shared");
        assertTrue(est >= 400, "expected >= 400, got " + est);
        assertTrue(est < 450, "expected close to 400, got " + est);
        assertEquals(400L, a.total());
    }

    @Test
    void disjointMergeTakesMaxAndUnderCountsOverlap() {
        // The documented precondition of mergeDisjointInto is that the shards
        // partition the key space. This pins what happens when it is violated,
        // so the trade-off is a tested fact rather than a caveat in a javadoc.
        CountMinSketch a = new CountMinSketch(5, 16384);
        CountMinSketch b = new CountMinSketch(5, 16384);
        for (int i = 0; i < 100; i++) a.add("shared");
        for (int i = 0; i < 300; i++) b.add("shared");
        Merge.mergeDisjointInto(a, b);
        int est = a.estimate("shared");
        assertTrue(est >= 300 && est < 400, "max of the two shards: " + est);
    }

    @Test
    void disjointMergeIsExactWhenThePreconditionHolds() {
        CountMinSketch a = new CountMinSketch(5, 16384);
        CountMinSketch b = new CountMinSketch(5, 16384);
        for (int i = 0; i < 100; i++) a.add("us-equities");
        for (int i = 0; i < 300; i++) b.add("eu-equities");
        Merge.mergeDisjointInto(a, b);
        assertEquals(100, a.estimate("us-equities"));
        assertEquals(300, a.estimate("eu-equities"));
    }

    @Test
    void mergeWithEmptyIsNoop() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch empty = new CountMinSketch(5, 4096);
        for (int i = 0; i < 50; i++) a.add("x");
        int before = a.estimate("x");
        Merge.mergeInto(a, empty);
        assertEquals(before, a.estimate("x"));
    }

    @Test
    void depthMismatchThrows() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(7, 4096);
        Merge.MergeException ex = assertThrows(
            Merge.MergeException.class,
            () -> Merge.mergeInto(a, b)
        );
        assertEquals("depth mismatch: dst=5, src=7", ex.getMessage());
    }

    @Test
    void widthMismatchThrows() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 8192);
        Merge.MergeException ex = assertThrows(
            Merge.MergeException.class,
            () -> Merge.mergeInto(a, b)
        );
        assertEquals("width mismatch: dst=4096, src=8192", ex.getMessage());
    }

    @Test
    void seedMismatchThrows() {
        // Different seeds mean different cells for the same key, so folding
        // the matrices would produce numbers that describe nothing.
        CountMinSketch a = new CountMinSketch(5, 4096, 1L);
        CountMinSketch b = new CountMinSketch(5, 4096, 2L);
        Merge.MergeException ex = assertThrows(
            Merge.MergeException.class,
            () -> Merge.mergeDisjointInto(a, b)
        );
        assertEquals("seed mismatch: dst=1, src=2", ex.getMessage());
    }

    @Test
    void disjointMergeOfEmptySrcIsIdempotent() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 4096);
        for (int i = 0; i < 50; i++) a.add("k");
        Merge.mergeDisjointInto(a, b);
        int once = a.estimate("k");
        Merge.mergeDisjointInto(a, b);
        assertEquals(once, a.estimate("k"));
    }

    @Test
    void mergedKeysOnlyInSrcBecomeVisible() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 4096);
        for (int i = 0; i < 75; i++) b.add("only-in-b");
        assertEquals(0, a.estimate("only-in-b"));
        Merge.mergeInto(a, b);
        assertTrue(a.estimate("only-in-b") >= 75);
    }

    @Test
    void fanInOfManyShardsBoundsTheUnion() {
        CountMinSketch sink = new CountMinSketch(5, 8192);
        for (int s = 0; s < 8; s++) {
            CountMinSketch shard = new CountMinSketch(5, 8192);
            for (int i = 0; i < 10 * (s + 1); i++) shard.add("ESZ5");
            Merge.mergeInto(sink, shard);
        }
        // 10 + 20 + ... + 80 = 360.
        assertTrue(sink.estimate("ESZ5") >= 360);
        assertEquals(360L, sink.total());
    }
}
