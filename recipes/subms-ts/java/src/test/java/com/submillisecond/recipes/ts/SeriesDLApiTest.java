package com.submillisecond.recipes.ts;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Optional;

import org.junit.jupiter.api.Test;

class SeriesDLApiTest {

    private static TsSeriesD seededD(long[] ts, double[] v) {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < ts.length; i++) s.push(ts[i], v[i]);
        return s;
    }

    @Test
    void seriesDFullSurface() {
        TsSeriesD s = seededD(new long[] {1, 2, 3, 4}, new double[] {5.0, 1.0, 9.0, 3.0});
        assertEquals(4, s.size());
        assertFalse(s.isEmpty());
        assertEquals(5.0, s.first().orElseThrow().value());
        assertEquals(3.0, s.last().orElseThrow().value());
        assertEquals(1.0, s.min().orElseThrow());
        assertEquals(9.0, s.max().orElseThrow());
        assertEquals(18.0, s.sum());
        assertEquals(4.5, s.mean().orElseThrow());
        assertEquals(4.5, s.meanOrNaN());
        assertEquals(1.0, s.rangeMin(2, 3).orElseThrow());
        assertEquals(9.0, s.rangeMax(2, 3).orElseThrow());
        assertEquals(10.0, s.rangeSum(2, 3));
        assertEquals(5.0, s.rangeMean(2, 3).orElseThrow());
        assertEquals(3.0, s.getAt(4).orElseThrow().value());
        assertEquals(2, s.nearest(2).orElseThrow().ts());
        assertEquals(3, s.nearestAfter(3).orElseThrow().ts());
        assertEquals(4, s.toList().size());
    }

    @Test
    void seriesDEmptyAndNanReject() {
        TsSeriesD s = new TsSeriesD();
        assertTrue(s.isEmpty());
        assertTrue(s.min().isEmpty());
        assertTrue(s.max().isEmpty());
        assertEquals(0.0, s.sum());
        assertTrue(s.mean().isEmpty());
        assertTrue(Double.isNaN(s.meanOrNaN()));
        assertTrue(s.rangeMin(0, 9).isEmpty());
        assertTrue(s.rangeMax(0, 9).isEmpty());
        assertEquals(0.0, s.rangeSum(0, 9));
        assertTrue(s.rangeMean(0, 9).isEmpty());
        assertTrue(s.rangeMean(9, 0).isEmpty());
        assertEquals(TsException.Kind.NULL_VALUE,
                assertThrows(TsException.class, () -> s.push(1, Double.NaN)).kind());
    }

    @Test
    void seriesDDeleteSurface() {
        TsSeriesD s = seededD(new long[] {1, 2, 3, 4, 5}, new double[] {1, 2, 3, 4, 5});
        assertEquals(3.0, s.deleteAt(3).orElseThrow().value());
        assertTrue(s.deleteAt(99).isEmpty());
        assertEquals(4, s.size());
        assertEquals(1, s.truncateBefore(2));
        assertEquals(1, s.truncateAfter(4));
        assertEquals(List.of(2.0, 4.0).size(), 2);
        s.clear();
        assertTrue(s.isEmpty());
    }

    @Test
    void seriesDFromPoints() {
        TsSeriesD s = TsSeriesD.fromPoints(List.of(new TsPoint<>(1L, 1.0), new TsPoint<>(2L, 2.0)));
        assertEquals(2, s.size());
        assertEquals(Optional.of("m"),
                s.withMetadata(TsSeriesMetadata.of(1, "m")).metadata().map(TsSeriesMetadata::name));
        TsSeriesMetadata meta = TsSeriesMetadata.of(2, "n");
        s.setMetadata(meta);
        assertEquals("n", s.metadata().orElseThrow().name());
    }

    @Test
    void seriesLSurface() {
        TsSeriesL s = TsSeriesL.withCapacity(8);
        for (long i = 1; i <= 5; i++) s.push(i, i * 10);
        assertEquals(5, s.size());
        assertFalse(s.isEmpty());
        assertEquals(10L, s.first().orElseThrow().value());
        assertEquals(50L, s.last().orElseThrow().value());
        assertEquals(30L, s.getAt(3).orElseThrow().value());
        assertEquals(20L, s.nearestBefore(2).orElseThrow().value());
        assertEquals(50L, s.max().orElseThrow());
        assertEquals(60L, s.rangeSum(1, 3));
        assertEquals(150.0, s.sum());
        assertEquals(30.0, s.meanOrNaN());
        assertEquals(List.of(2L, 3L), s.rangeTimestamps(2, 3));
        assertTrue(s.rangeTimestamps(9, 1).isEmpty());
        assertEquals(0L, s.rangeSum(9, 1));
        assertEquals(TsException.Kind.NOT_MONOTONIC,
                assertThrows(TsException.class, () -> s.push(0, 1)).kind());
    }

    @Test
    void seriesLEmpty() {
        TsSeriesL s = new TsSeriesL();
        assertTrue(s.isEmpty());
        assertTrue(s.first().isEmpty());
        assertTrue(s.last().isEmpty());
        assertTrue(s.max().isEmpty());
        assertTrue(Double.isNaN(s.meanOrNaN()));
        assertEquals(0, s.deleteRange(1, 0));
    }
}
