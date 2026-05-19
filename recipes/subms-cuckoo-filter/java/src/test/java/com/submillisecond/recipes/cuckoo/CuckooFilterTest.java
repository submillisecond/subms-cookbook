package com.submillisecond.recipes.cuckoo;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CuckooFilterTest {

    @Test
    void insertContainsDelete() {
        CuckooFilter cf = new CuckooFilter(1000);
        for (int i = 0; i < 500; i++) assertTrue(cf.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.contains("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.delete("k" + i));
        for (int i = 0; i < 500; i++) assertFalse(cf.contains("k" + i));
        assertEquals(0, cf.size());
    }

    @Test
    void deleteNonexistent() {
        CuckooFilter cf = new CuckooFilter(100);
        assertFalse(cf.delete("never"));
    }

    @Test
    void emptyContains() {
        assertFalse(new CuckooFilter(100).contains("x"));
    }

    @Test
    void falsePositiveRateUnderThreePercent() {
        int n = 10_000;
        CuckooFilter cf = new CuckooFilter(n);
        for (int i = 0; i < n; i++) cf.insert("present" + i);
        int probes = 10_000;
        int fp = 0;
        for (int i = 0; i < probes; i++) if (cf.contains("absent" + i)) fp++;
        assertTrue(((double) fp / probes) < 0.03, "fpr too high: " + fp);
    }

    @Test
    void bucketCountIsPowerOfTwo() {
        int n = new CuckooFilter(1000).bucketCount();
        assertEquals(Integer.highestOneBit(n), n);
    }

    @Test
    void sizeTracksInsertsAndDeletes() {
        CuckooFilter cf = new CuckooFilter(1000);
        assertEquals(0, cf.size());
        cf.insert("a");
        cf.insert("b");
        assertEquals(2, cf.size());
        cf.delete("a");
        assertEquals(1, cf.size());
        cf.delete("absent");
        assertEquals(1, cf.size());
    }

    @Test
    void isEmptyInitially() {
        assertTrue(new CuckooFilter(100).isEmpty());
    }

    @Test
    void duplicateInsertsIncreaseCount() {
        CuckooFilter cf = new CuckooFilter(100);
        cf.insert("dup");
        cf.insert("dup");
        cf.insert("dup");
        assertEquals(3, cf.size());
        assertTrue(cf.contains("dup"));
        cf.delete("dup");
        cf.delete("dup");
        cf.delete("dup");
        assertFalse(cf.contains("dup"));
        assertEquals(0, cf.size());
    }

    @Test
    void stressInsertContainsDeleteCycle() {
        CuckooFilter cf = new CuckooFilter(2000);
        for (int cycle = 0; cycle < 3; cycle++) {
            for (int i = 0; i < 1000; i++) cf.insert("cycle" + cycle + "-k" + i);
            for (int i = 0; i < 1000; i++) assertTrue(cf.contains("cycle" + cycle + "-k" + i));
            for (int i = 0; i < 1000; i++) cf.delete("cycle" + cycle + "-k" + i);
        }
        assertEquals(0, cf.size());
    }
}
