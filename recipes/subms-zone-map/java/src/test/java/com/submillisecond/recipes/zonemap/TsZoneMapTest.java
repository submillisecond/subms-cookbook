package com.submillisecond.recipes.zonemap;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Optional;
import java.util.function.LongToDoubleFunction;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.gorillablock.TsGorillaBlock;

final class TsZoneMapTest {

    private static TsGorillaBlock block(long base, long n, LongToDoubleFunction f) {
        TsGorillaBlock b = new TsGorillaBlock();
        for (long i = 0; i < n; i++) {
            b.append(base + i, f.applyAsDouble(i));
        }
        return b;
    }

    private static Optional<TsValuePredicate> pred(TsValueOp op, double rhs) {
        return Optional.of(TsValuePredicate.of(op, rhs));
    }

    @Test
    void observeRecordsStats() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(1_000, 100, i -> i));
        assertEquals(1, z.len());
        TsZone zone = z.zones().get(0);
        assertEquals(1, zone.blockId());
        assertEquals(1_000, zone.tsMin());
        assertEquals(1_099, zone.tsMax());
        assertEquals(0.0, zone.valueMin());
        assertEquals(99.0, zone.valueMax());
        assertEquals(100, zone.count());
    }

    @Test
    void emptyBlockSkipped() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, new TsGorillaBlock());
        assertTrue(z.isEmpty());
    }

    @Test
    void timeWindowPruning() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(0, 100, i -> i));
        z.observe(2, block(1_000, 100, i -> i));
        z.observe(3, block(2_000, 100, i -> i));
        assertArrayEquals(new long[] {2}, z.candidates(1_000, 1_050));
        assertArrayEquals(new long[] {1, 2, 3}, z.candidates(50, 2_050));
        assertEquals(0, z.candidates(5_000, 6_000).length);
        assertArrayEquals(new long[] {2, 3}, z.candidates(1_099, 2_000));
    }

    @Test
    void loGtHiReturnsEmpty() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(0, 10, i -> i));
        assertEquals(0, z.candidates(100, 0).length);
    }

    @Test
    void valuePredicatePruning() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(0, 100, i -> i));
        z.observe(2, block(1_000, 100, i -> 100.0 + i));

        assertArrayEquals(new long[] {2}, z.candidates(0, 2_000, pred(TsValueOp.GT, 150.0)));
        assertArrayEquals(new long[] {1}, z.candidates(0, 2_000, pred(TsValueOp.LT, 50.0)));
        assertArrayEquals(new long[] {1}, z.candidates(0, 2_000, pred(TsValueOp.EQ, 99.0)));
        assertEquals(0, z.candidates(0, 2_000, pred(TsValueOp.GE, 200.0)).length);
        assertArrayEquals(new long[] {1, 2}, z.candidates(0, 2_000, pred(TsValueOp.LE, 199.0)));
    }

    @Test
    void combinedTimeAndValue() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(0, 100, i -> i));
        z.observe(2, block(1_000, 100, i -> 100.0 + i));
        assertEquals(0, z.candidates(0, 99, pred(TsValueOp.GT, 150.0)).length);
    }

    @Test
    void observeZoneDirect() {
        TsZoneMap z = new TsZoneMap();
        z.observeZone(new TsZone(42, 10, 20, 1.0, 5.0, 11));
        assertArrayEquals(new long[] {42}, z.candidates(15, 18));
    }

    @Test
    void prunesLargeIndexFast() {
        TsZoneMap z = TsZoneMap.withCapacity(100_000);
        for (long id = 0; id < 100_000L; id++) {
            long base = id * 1_000L;
            z.observeZone(new TsZone(id, base, base + 999, 0.0, id, 1_000));
        }
        assertArrayEquals(new long[] {5, 6, 7}, z.candidates(5_000, 7_500));
    }

    @Test
    void clearResets() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(0, 10, i -> i));
        z.clear();
        assertTrue(z.isEmpty());
        assertEquals(0, z.candidates(0, 100).length);
    }

    @Test
    void predicateSatisfiableBoundaries() {
        TsValuePredicate gt = TsValuePredicate.of(TsValueOp.GT, 10.0);
        assertFalse(gt.satisfiable(0.0, 10.0));
        assertTrue(gt.satisfiable(0.0, 10.5));

        TsValuePredicate ge = TsValuePredicate.of(TsValueOp.GE, 10.0);
        assertTrue(ge.satisfiable(0.0, 10.0));
        assertFalse(ge.satisfiable(0.0, 9.5));

        TsValuePredicate lt = TsValuePredicate.of(TsValueOp.LT, 10.0);
        assertFalse(lt.satisfiable(10.0, 20.0));
        assertTrue(lt.satisfiable(9.5, 20.0));

        TsValuePredicate le = TsValuePredicate.of(TsValueOp.LE, 10.0);
        assertTrue(le.satisfiable(10.0, 20.0));
        assertFalse(le.satisfiable(10.5, 20.0));

        TsValuePredicate eq = TsValuePredicate.of(TsValueOp.EQ, 10.0);
        assertTrue(eq.satisfiable(5.0, 15.0));
        assertTrue(eq.satisfiable(10.0, 10.0));
        assertFalse(eq.satisfiable(11.0, 15.0));
        assertFalse(eq.satisfiable(5.0, 9.0));
    }

    @Test
    void valuePredicateRecordAccessors() {
        TsValuePredicate p = TsValuePredicate.of(TsValueOp.GE, 3.5);
        assertEquals(TsValueOp.GE, p.op());
        assertEquals(3.5, p.rhs());
    }

    @Test
    void valueOpEnumComplete() {
        assertEquals(5, TsValueOp.values().length);
        assertEquals(TsValueOp.EQ, TsValueOp.valueOf("EQ"));
    }

    @Test
    void observationOrderPreserved() {
        TsZoneMap z = new TsZoneMap();
        z.observe(9, block(0, 10, i -> i));
        z.observe(4, block(5, 10, i -> i));
        z.observe(7, block(8, 10, i -> i));
        assertArrayEquals(new long[] {9, 4, 7}, z.candidates(0, 100));
    }

    @Test
    void noPredicateOverloadMatchesEmptyOptional() {
        TsZoneMap z = new TsZoneMap();
        z.observe(1, block(0, 100, i -> i));
        assertArrayEquals(z.candidates(0, 50), z.candidates(0, 50, Optional.empty()));
    }
}
