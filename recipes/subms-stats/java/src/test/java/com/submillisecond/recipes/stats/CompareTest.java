package com.submillisecond.recipes.stats;

import org.junit.jupiter.api.Test;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CompareTest {

    @Test
    void ksSameDistributionNearZero() {
        long[] a = new long[1000];
        long[] b = new long[1000];
        for (int i = 0; i < 1000; i++) { a[i] = i; b[i] = i; }
        assertTrue(Compare.ksStatistic(a, b).get() < 0.01);
    }

    @Test
    void ksShiftedDistributionLarge() {
        long[] a = new long[1000];
        long[] b = new long[1000];
        for (int i = 0; i < 1000; i++) { a[i] = i; b[i] = i + 500; }
        assertTrue(Compare.ksStatistic(a, b).get() > 0.4);
    }

    @Test
    void ksEmptyReturnsEmpty() {
        assertFalse(Compare.ksStatistic(new long[0], new long[]{1, 2}).isPresent());
        assertFalse(Compare.ksStatistic(new long[]{1, 2}, new long[0]).isPresent());
    }

    @Test
    void cohensDZeroForIdentical() {
        long[] a = new long[100];
        long[] b = new long[100];
        for (int i = 0; i < 100; i++) { a[i] = i; b[i] = i; }
        Optional<Double> d = Compare.cohensD(a, b);
        assertTrue(d.isPresent());
        assertTrue(Math.abs(d.get()) < 0.01);
    }

    @Test
    void cohensDPositiveWhenCandidateSlower() {
        long[] a = new long[100];
        long[] b = new long[100];
        for (int i = 0; i < 100; i++) { a[i] = i + 100; b[i] = i + 200; }
        Optional<Double> d = Compare.cohensD(a, b);
        assertTrue(d.isPresent());
        assertTrue(d.get() > 0.5);
    }
}
