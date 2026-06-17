package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;
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
}
