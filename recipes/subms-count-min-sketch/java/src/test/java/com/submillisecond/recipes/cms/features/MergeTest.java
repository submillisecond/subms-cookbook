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
    void mergeSharedKeyTakesMaxNotSum() {
        CountMinSketch a = new CountMinSketch(5, 16384);
        CountMinSketch b = new CountMinSketch(5, 16384);
        for (int i = 0; i < 100; i++) a.add("shared");
        for (int i = 0; i < 300; i++) b.add("shared");
        Merge.mergeInto(a, b);
        int est = a.estimate("shared");
        assertTrue(est >= 300, "expected >= 300, got " + est);
        assertTrue(est < 350, "expected close to 300, got " + est);
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
        assertTrue(ex.getMessage().contains("depth"));
    }

    @Test
    void widthMismatchThrows() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 8192);
        Merge.MergeException ex = assertThrows(
            Merge.MergeException.class,
            () -> Merge.mergeInto(a, b)
        );
        assertTrue(ex.getMessage().contains("width"));
    }

    @Test
    void mergeIsIdempotentWhenSrcAlreadyDominated() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 4096);
        for (int i = 0; i < 50; i++) a.add("k");
        Merge.mergeInto(a, b);
        int once = a.estimate("k");
        Merge.mergeInto(a, b);
        int twice = a.estimate("k");
        assertEquals(once, twice);
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
}
