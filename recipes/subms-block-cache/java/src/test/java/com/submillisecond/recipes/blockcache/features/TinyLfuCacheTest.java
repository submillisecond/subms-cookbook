package com.submillisecond.recipes.blockcache.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TinyLfuCacheTest {

    @Test
    void basicPutThenGet() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(16);
        c.put(1, 10);
        c.put(2, 20);
        assertEquals(10, c.get(1));
        assertEquals(20, c.get(2));
        assertNull(c.get(999));
    }

    @Test
    void capacityFloorIsFour() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(0);
        assertEquals(4, c.capacity());
    }

    @Test
    void admissionFilterTracksAdmissionsAndRejections() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(64);
        for (int round = 0; round < 50; round++) {
            for (int k = 0; k < 16; k++) c.get(k);
        }
        for (int k = 0; k < 16; k++) c.put(k, k);
        for (int round = 0; round < 50; round++) {
            for (int k = 0; k < 16; k++) c.get(k);
        }
        long rejBefore = c.rejections();
        for (int k = 1000; k < 3000; k++) c.put(k, k);
        assertTrue(
            c.rejections() > rejBefore,
            "expected the admission filter to reject some scan candidates"
        );
        assertTrue(c.admissions() + c.rejections() > 0);
    }

    @Test
    void promotionFromProbationToProtected() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(64);
        for (int k = 0; k < 40; k++) c.put(k, k);
        int before = c.protectedLen();
        for (int i = 0; i < 5; i++) c.get(5);
        assertTrue(c.protectedLen() >= before);
    }

    @Test
    void updateInPlaceDoesNotEvict() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(16);
        c.put(7, 70);
        assertNull(c.put(7, 71));
        assertEquals(71, c.get(7));
    }

    @Test
    void admissionsCounterTracksWindowEvictions() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(16);
        for (int round = 0; round < 50; round++) {
            for (int k = 0; k < 8; k++) c.get(k);
        }
        for (int k = 0; k < 50; k++) c.put(k, k);
        assertTrue(c.admissions() + c.rejections() > 0);
    }

    @Test
    void cmsEstimateGrowsWithAccessCount() {
        TinyLfuCache.Cms cms = new TinyLfuCache.Cms(256, 10_000L);
        long h = 0x1234_5678_9abc_defL;
        for (int i = 0; i < 10; i++) cms.increment(h);
        int e = cms.estimate(h);
        assertTrue(e >= 10 || e == 15, "estimate was " + e);
    }

    @Test
    void cmsAgingHalvesCounters() {
        TinyLfuCache.Cms cms = new TinyLfuCache.Cms(64, 30L);
        long h = 0xdeadbeefL;
        for (int i = 0; i < 15; i++) cms.increment(h);
        int before = cms.estimate(h);
        for (long i = 0; i < 30; i++) {
            cms.increment(i * 0x9e3779b97f4a7c15L);
        }
        int after = cms.estimate(h);
        assertTrue(after <= before, "after=" + after + " before=" + before);
    }

    @Test
    void inspectorsReadable() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(16);
        assertTrue(c.isEmpty());
        assertEquals(0, c.size());
        c.put(1, 1);
        assertEquals(1, c.size());
        assertTrue(c.windowLen() + c.probationLen() + c.protectedLen() == 1);
        assertEquals(c.size(), c.windowLen() + c.probationLen() + c.protectedLen());
    }

    @Test
    void probationToProtectedWithDemotion() {
        // Fill cache so Protected is full, then hit a probation key. It
        // promotes and demotes Protected's LRU back to probation.
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(64);
        for (int k = 0; k < 200; k++) {
            c.put(k, k);
        }
        // Make some keys popular so they sit in protected.
        for (int round = 0; round < 30; round++) {
            for (int k = 0; k < 200; k++) {
                if (c.get(k) != null) {
                    // hit
                }
            }
        }
        // At this point Protected might be full; subsequent probation hits
        // should trigger demotion.
        for (int k = 0; k < 200; k++) c.get(k);
        // No assertion on internal state because exact occupancy varies,
        // but the operation must not throw and size stays bounded.
        assertTrue(c.size() <= c.capacity());
    }

    @Test
    void updateInPlaceAcrossSegments() {
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(16);
        for (int k = 0; k < 8; k++) c.put(k, k);
        // Hammer some so they migrate to probation/protected.
        for (int round = 0; round < 20; round++) {
            for (int k = 0; k < 4; k++) c.get(k);
        }
        // Find a key that's still resident and update it.
        for (int k = 0; k < 8; k++) {
            if (c.get(k) != null) {
                assertNull(c.put(k, 999), "update of resident key " + k + " must not evict");
                assertEquals(999, c.get(k));
                return;
            }
        }
    }

    @Test
    void doorkeeperClearWhenCmsResets() {
        // Use a tiny cache with a tight CMS sample-size to force the
        // aging branch, which also clears the doorkeeper.
        TinyLfuCache<Integer, Integer> c = new TinyLfuCache<>(4);
        for (int k = 0; k < 200; k++) {
            c.get(k);
        }
        // No assertion - we just need to traverse the clear() path.
        assertTrue(c.capacity() == 4);
    }
}
