package com.submillisecond.stats;

import org.junit.jupiter.api.Test;

import java.util.Arrays;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class BootstrapTest {

    @Test
    void p99CiBracketsPointEstimate() {
        long[] v = new long[1000];
        for (int i = 0; i < 1000; i++) v[i] = i;
        Bootstrap.CI ci = Bootstrap.bootstrapPercentileCi(v, 0.99, 200, 0.95, 42L);
        long[] sorted = v.clone();
        Arrays.sort(sorted);
        long point = Percentiles.percentile(sorted, 0.99);
        assertTrue(ci.lo() <= point && point <= ci.hi(),
                "CI [" + ci.lo() + ", " + ci.hi() + "] brackets " + point);
    }

    @Test
    void emptyReturnsZeroPair() {
        Bootstrap.CI ci = Bootstrap.bootstrapPercentileCi(new long[0], 0.99, 100, 0.95, 0L);
        assertEquals(0L, ci.lo());
        assertEquals(0L, ci.hi());
    }

    @Test
    void deterministicUnderSameSeed() {
        long[] v = new long[200];
        for (int i = 0; i < 200; i++) v[i] = i;
        Bootstrap.CI a = Bootstrap.bootstrapPercentileCi(v, 0.99, 100, 0.95, 7L);
        Bootstrap.CI b = Bootstrap.bootstrapPercentileCi(v, 0.99, 100, 0.95, 7L);
        assertEquals(a, b);
    }

    @Test
    void zeroItersReturnsZeroPair() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        Bootstrap.CI ci = Bootstrap.bootstrapPercentileCi(v, 0.99, 0, 0.95, 0L);
        assertEquals(0L, ci.lo());
        assertEquals(0L, ci.hi());
    }
}
