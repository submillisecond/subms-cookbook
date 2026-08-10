package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class VariableFpCuckooFilterTest {

    @Test
    void roundTripEightBit() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1000, FingerprintWidth.EIGHT);
        for (int i = 0; i < 500; i++) assertTrue(cf.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.contains("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.delete("k" + i));
        assertEquals(0, cf.size());
    }

    @Test
    void roundTripTwelveBit() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1000, FingerprintWidth.TWELVE);
        for (int i = 0; i < 500; i++) assertTrue(cf.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.contains("k" + i));
        assertEquals(500, cf.size());
    }

    @Test
    void roundTripSixteenBit() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1000, FingerprintWidth.SIXTEEN);
        for (int i = 0; i < 500; i++) assertTrue(cf.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.contains("k" + i));
    }

    @Test
    void widerFingerprintLowersFpr() {
        int n = 5_000;
        int probes = 10_000;
        VariableFpCuckooFilter narrow = new VariableFpCuckooFilter(n, FingerprintWidth.EIGHT);
        VariableFpCuckooFilter wide = new VariableFpCuckooFilter(n, FingerprintWidth.SIXTEEN);
        for (int i = 0; i < n; i++) {
            narrow.insert("present" + i);
            wide.insert("present" + i);
        }
        int narrowFp = 0, wideFp = 0;
        for (int i = 0; i < probes; i++) {
            String k = "absent" + i;
            if (narrow.contains(k)) narrowFp++;
            if (wide.contains(k)) wideFp++;
        }
        assertTrue(wideFp < narrowFp, "wideFp=" + wideFp + " narrowFp=" + narrowFp);
    }

    @Test
    void emptyFilterRejectsEverything() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(100, FingerprintWidth.TWELVE);
        assertFalse(cf.contains("never-inserted"));
        assertTrue(cf.isEmpty());
    }

    @Test
    void widthAccessorReportsConfiguredValue() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(100, FingerprintWidth.TWELVE);
        assertEquals(FingerprintWidth.TWELVE, cf.width());
        assertEquals(12, cf.width().bits());
    }

    @Test
    void deleteUnknownIsFalse() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(100, FingerprintWidth.SIXTEEN);
        assertFalse(cf.delete("never-inserted"));
    }

    @Test
    void bucketCountIsPowerOfTwo() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1000, FingerprintWidth.TWELVE);
        int n = cf.bucketCount();
        assertEquals(0, n & (n - 1));
    }

    @Test
    void saturationNeverProducesAFalseNegative() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1, FingerprintWidth.SIXTEEN);
        List<String> accepted = new ArrayList<>();
        for (int i = 0; i < 4096; i++) {
            String key = "k" + i;
            if (cf.insert(key)) accepted.add(key);
        }
        assertTrue(accepted.size() < 4096, "a 2-bucket filter must refuse");
        for (String key : accepted) {
            assertTrue(cf.contains(key), key + " was accepted then lost");
        }
    }

    @Test
    void victimIsRehomedOnceADeleteFreesASlot() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1, FingerprintWidth.TWELVE);
        List<String> accepted = new ArrayList<>();
        for (int i = 0; i < 4096; i++) {
            String key = "k" + i;
            if (!cf.insert(key)) break;
            accepted.add(key);
        }
        assertFalse(cf.insert("blocked"));
        assertTrue(cf.delete(accepted.get(0)));
        assertTrue(cf.insert("blocked"));
        assertTrue(cf.contains("blocked"));
    }

    @Test
    void clearResetsToEmptyAndKeepsGeometry() {
        VariableFpCuckooFilter cf = new VariableFpCuckooFilter(1000, FingerprintWidth.TWELVE);
        int buckets = cf.bucketCount();
        for (int i = 0; i < 300; i++) cf.insert("k" + i);
        cf.clear();
        assertTrue(cf.isEmpty());
        assertEquals(buckets, cf.bucketCount());
        assertFalse(cf.contains("k1"));
    }

    @Test
    void estimatedFppFallsAsTheFingerprintWidens() {
        int n = 2_000;
        VariableFpCuckooFilter narrow = new VariableFpCuckooFilter(n, FingerprintWidth.EIGHT);
        VariableFpCuckooFilter wide = new VariableFpCuckooFilter(n, FingerprintWidth.SIXTEEN);
        assertEquals(0.0, narrow.estimatedFpp());
        for (int i = 0; i < n; i++) {
            narrow.insert("k" + i);
            wide.insert("k" + i);
        }
        assertTrue(wide.estimatedFpp() < narrow.estimatedFpp() / 100.0);
    }
}
