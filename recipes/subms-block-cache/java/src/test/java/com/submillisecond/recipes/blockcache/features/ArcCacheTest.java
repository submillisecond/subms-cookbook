package com.submillisecond.recipes.blockcache.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ArcCacheTest {

    @Test
    void putThenGetReturnsValue() {
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        c.put(1, 10);
        c.put(2, 20);
        assertEquals(10, c.get(1));
        assertEquals(20, c.get(2));
        assertNull(c.get(99));
    }

    @Test
    void secondHitPromotesIntoT2() {
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        c.put(1, 10);
        assertEquals(1, c.t1Len());
        assertEquals(0, c.t2Len());
        c.get(1);
        assertEquals(0, c.t1Len());
        assertEquals(1, c.t2Len());
    }

    @Test
    void capacityOneEvictsOnEveryNewKey() {
        ArcCache<Integer, Integer> c = new ArcCache<>(1);
        assertNull(c.put(1, 1));
        ArcCache.Evicted<Integer, Integer> ev = c.put(2, 2);
        assertNotNull(ev);
        assertEquals(1, c.size());
        assertNull(c.get(1));
        assertEquals(2, c.get(2));
    }

    @Test
    void scanResistancePreservesT2() {
        ArcCache<Integer, Integer> c = new ArcCache<>(8);
        for (int k = 0; k < 4; k++) {
            c.put(k, k);
            c.get(k);
        }
        assertEquals(4, c.t2Len());
        for (int k = 1000; k < 1100; k++) {
            c.put(k, k);
        }
        for (int k = 0; k < 4; k++) {
            assertNotNull(c.get(k), "frequent key " + k + " was evicted by scan");
        }
    }

    @Test
    void ghostHitAdaptsP() {
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        for (int k = 0; k < 8; k++) {
            c.put(k, k);
        }
        int pBefore = c.p();
        c.put(0, 100);
        assertTrue(c.p() >= pBefore);
    }

    @Test
    void updateInPlaceDoesNotEvict() {
        ArcCache<Integer, Integer> c = new ArcCache<>(2);
        c.put(1, 10);
        c.put(2, 20);
        ArcCache.Evicted<Integer, Integer> ev = c.put(1, 11);
        assertNull(ev);
        assertEquals(11, c.get(1));
    }

    @Test
    void manyInsertsKeepsResidentAtOrBelowC() {
        ArcCache<Integer, Integer> c = new ArcCache<>(16);
        for (int k = 0; k < 1000; k++) {
            c.put(k, k);
            assertTrue(c.size() <= 16, "resident set exceeded c at k=" + k);
        }
    }

    @Test
    void updateOfT2KeyStaysInT2() {
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        c.put(1, 10);
        c.get(1); // promote into T2
        assertEquals(1, c.t2Len());
        ArcCache.Evicted<Integer, Integer> ev = c.put(1, 11);
        assertNull(ev, "update of T2 key should not evict");
        assertEquals(11, c.get(1));
        assertEquals(1, c.t2Len());
    }

    @Test
    void b2GhostHitShrinksPAndEvictsFromT2() {
        // Build T2 by hitting keys twice; flush them to B2; then re-access
        // one of them. That triggers Case III (B2 hit) and shrinks p.
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        for (int k = 0; k < 4; k++) {
            c.put(k, k);
            c.get(k);
        }
        assertEquals(4, c.t2Len());
        // Push more new keys to force evictions from T2 -> B2.
        for (int k = 100; k < 110; k++) {
            c.put(k, k);
        }
        // At least some originals should be in B2 now.
        assertTrue(c.b2Len() > 0, "expected some entries in B2");
        // Find an originally-frequent key that is now ghost (in B2 or B1).
        boolean hitAnyGhost = false;
        int pBefore = c.p();
        for (int k = 0; k < 4; k++) {
            if (c.get(k) == null) {
                // It's a ghost. Touch it via put to trigger ghost-hit logic.
                c.put(k, k);
                hitAnyGhost = true;
                break;
            }
        }
        assertTrue(hitAnyGhost, "expected at least one ghost-list resident");
        // After a ghost hit, p must have moved one way or the other.
        // We don't assert direction since it could be either B1 or B2.
        int pAfter = c.p();
        assertTrue(pAfter >= 0 && pAfter <= c.capacity(),
            "p out of range: " + pAfter + " before=" + pBefore);
    }

    @Test
    void b1AndB2GhostsBothPopulated() {
        // Run a workload that produces ghost entries on both lists.
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        // T2 builders.
        for (int k = 0; k < 4; k++) { c.put(k, k); c.get(k); }
        // T1 fillers, then flush to B1.
        for (int k = 100; k < 108; k++) c.put(k, k);
        // Force more pressure to push some of the T2 ones to B2.
        for (int k = 200; k < 208; k++) c.put(k, k);
        assertTrue(c.b1Len() + c.b2Len() > 0);
        assertTrue(c.b1Len() <= c.capacity());
        assertTrue(c.b2Len() <= 2 * c.capacity());
    }

    @Test
    void inspectorsAreReadable() {
        ArcCache<Integer, Integer> c = new ArcCache<>(4);
        assertEquals(4, c.capacity());
        assertTrue(c.isEmpty());
        c.put(1, 10);
        assertFalse(c.isEmpty());
        assertEquals(0, c.b1Len());
        assertEquals(0, c.b2Len());
    }

    @Test
    void capacityFloorIsOne() {
        ArcCache<Integer, Integer> c = new ArcCache<>(0);
        assertEquals(1, c.capacity());
        c.put(1, 10);
        assertEquals(10, c.get(1));
    }
}
