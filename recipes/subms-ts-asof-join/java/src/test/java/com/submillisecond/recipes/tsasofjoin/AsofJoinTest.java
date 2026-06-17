package com.submillisecond.recipes.tsasofjoin;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.OptionalLong;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsSeries;

class AsofJoinTest {

    private static TsSeries<Double> series(long[][] pts) {
        TsSeries<Double> s = new TsSeries<>();
        for (long[] p : pts) {
            s.push(p[0], (double) p[1]);
        }
        return s;
    }

    private static OptionalLong rightTs(AsofJoin.TsMatch m) {
        return m.right().map(p -> OptionalLong.of(p.ts())).orElse(OptionalLong.empty());
    }

    @Test
    void backwardBasic() {
        TsSeries<Double> l = series(new long[][] {{10, 1}, {25, 2}, {40, 3}});
        TsSeries<Double> r = series(new long[][] {{5, 99}, {20, 98}, {30, 97}});
        List<AsofJoin.TsMatch> m = AsofJoin.backward(l, r);
        assertEquals(3, m.size());
        assertEquals(OptionalLong.of(5), rightTs(m.get(0)));
        assertEquals(OptionalLong.of(20), rightTs(m.get(1)));
        assertEquals(OptionalLong.of(30), rightTs(m.get(2)));
    }

    @Test
    void backwardNoMatchBeforeFirst() {
        TsSeries<Double> l = series(new long[][] {{1, 1}, {10, 2}});
        TsSeries<Double> r = series(new long[][] {{5, 99}});
        List<AsofJoin.TsMatch> m = AsofJoin.backward(l, r);
        assertTrue(m.get(0).right().isEmpty());
        assertEquals(OptionalLong.of(5), rightTs(m.get(1)));
    }

    @Test
    void forwardBasic() {
        TsSeries<Double> l = series(new long[][] {{10, 1}, {25, 2}, {40, 3}});
        TsSeries<Double> r = series(new long[][] {{5, 99}, {20, 98}, {30, 97}});
        List<AsofJoin.TsMatch> m = AsofJoin.forward(l, r);
        assertEquals(OptionalLong.of(20), rightTs(m.get(0)));
        assertEquals(OptionalLong.of(30), rightTs(m.get(1)));
        assertTrue(m.get(2).right().isEmpty());
    }

    @Test
    void forwardExactMatch() {
        TsSeries<Double> l = series(new long[][] {{20, 1}});
        TsSeries<Double> r = series(new long[][] {{20, 99}});
        assertEquals(OptionalLong.of(20), rightTs(AsofJoin.forward(l, r).get(0)));
        assertEquals(OptionalLong.of(20), rightTs(AsofJoin.backward(l, r).get(0)));
    }

    @Test
    void nearestWithinTolerance() {
        TsSeries<Double> l = series(new long[][] {{10, 1}, {100, 2}});
        TsSeries<Double> r = series(new long[][] {{8, 99}, {40, 98}});
        List<AsofJoin.TsMatch> m = AsofJoin.nearest(l, r, 5);
        assertEquals(OptionalLong.of(8), rightTs(m.get(0)));
        assertTrue(m.get(1).right().isEmpty());
    }

    @Test
    void nearestPicksCloserSide() {
        TsSeries<Double> l = series(new long[][] {{50, 1}});
        TsSeries<Double> r = series(new long[][] {{40, 99}, {58, 98}});
        List<AsofJoin.TsMatch> m = AsofJoin.nearest(l, r, 100);
        assertEquals(OptionalLong.of(58), rightTs(m.get(0)));
    }

    @Test
    void nearestTieResolvesEarlier() {
        TsSeries<Double> l = series(new long[][] {{50, 1}});
        TsSeries<Double> r = series(new long[][] {{45, 99}, {55, 98}});
        List<AsofJoin.TsMatch> m = AsofJoin.nearest(l, r, 100);
        assertEquals(OptionalLong.of(45), rightTs(m.get(0)));
    }

    @Test
    void emptyRightAllNone() {
        TsSeries<Double> l = series(new long[][] {{1, 1}, {2, 2}});
        TsSeries<Double> r = new TsSeries<>();
        for (AsofJoin.TsMatch m : AsofJoin.backward(l, r)) {
            assertTrue(m.right().isEmpty());
        }
        assertEquals(2, AsofJoin.forward(l, r).size());
    }

    @Test
    void emptyLeftEmptyResult() {
        TsSeries<Double> l = new TsSeries<>();
        TsSeries<Double> r = series(new long[][] {{1, 1}});
        assertTrue(AsofJoin.backward(l, r).isEmpty());
        assertTrue(AsofJoin.forward(l, r).isEmpty());
        assertTrue(AsofJoin.nearest(l, r, 10).isEmpty());
    }

    @Test
    void leftValuesPreserved() {
        TsSeries<Double> l = series(new long[][] {});
        l.push(10, 1.5);
        l.push(20, 2.5);
        TsSeries<Double> r = series(new long[][] {{5, 9}});
        List<AsofJoin.TsMatch> m = AsofJoin.backward(l, r);
        assertEquals(1.5, m.get(0).left().value());
        assertEquals(2.5, m.get(1).left().value());
    }

    @Test
    void nearestEmptyRightAllNone() {
        TsSeries<Double> l = series(new long[][] {{1, 1}, {2, 2}});
        TsSeries<Double> r = new TsSeries<>();
        List<AsofJoin.TsMatch> m = AsofJoin.nearest(l, r, 1_000);
        assertEquals(2, m.size());
        for (AsofJoin.TsMatch row : m) {
            assertTrue(row.right().isEmpty());
        }
    }

    @Test
    void denseMergeWalkMatchesBruteForce() {
        List<long[]> lp = new ArrayList<>();
        List<long[]> rp = new ArrayList<>();
        for (long i = 0; i < 1_000; i++) {
            lp.add(new long[] {i * 3, i});
            rp.add(new long[] {i * 2, i});
        }
        TsSeries<Double> l = series(lp.toArray(new long[0][]));
        TsSeries<Double> r = series(rp.toArray(new long[0][]));
        List<AsofJoin.TsMatch> m = AsofJoin.backward(l, r);
        for (AsofJoin.TsMatch row : m) {
            OptionalLong expect = OptionalLong.empty();
            for (long[] p : rp) {
                if (p[0] <= row.left().ts()) {
                    expect = OptionalLong.of(p[0]);
                }
            }
            assertEquals(expect, rightTs(row), "left ts " + row.left().ts());
        }
        assertFalse(m.isEmpty());
    }
}
