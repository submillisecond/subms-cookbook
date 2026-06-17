package com.submillisecond.recipes.tsfill;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

class FillTest {

    private static TsSeries<Double> series(double[][] pts) {
        TsSeries<Double> s = new TsSeries<>();
        for (double[] p : pts) {
            s.push((long) p[0], p[1]);
        }
        return s;
    }

    private static List<long[]> pairsTs(TsSeries<Double> s) {
        List<long[]> out = new ArrayList<>();
        for (TsPoint<Double> p : s) {
            out.add(new long[] {p.ts()});
        }
        return out;
    }

    private static void assertPairs(TsSeries<Double> s, double[][] expect) {
        List<TsPoint<Double>> got = new ArrayList<>();
        for (TsPoint<Double> p : s) {
            got.add(p);
        }
        assertEquals(expect.length, got.size(), "point count");
        for (int i = 0; i < expect.length; i++) {
            assertEquals((long) expect[i][0], got.get(i).ts(), "ts at " + i);
            assertEquals(expect[i][1], got.get(i).value(), 1e-9, "value at " + i);
        }
    }

    @Test
    void linearFillsGap() {
        TsSeries<Double> s = series(new double[][] {{0, 0.0}, {40, 4.0}});
        assertPairs(Fill.linear(s, 10),
                new double[][] {{0, 0.0}, {10, 1.0}, {20, 2.0}, {30, 3.0}, {40, 4.0}});
    }

    @Test
    void locfCarriesLeft() {
        TsSeries<Double> s = series(new double[][] {{0, 7.0}, {30, 9.0}});
        assertPairs(Fill.locf(s, 10),
                new double[][] {{0, 7.0}, {10, 7.0}, {20, 7.0}, {30, 9.0}});
    }

    @Test
    void zeroFillsGap() {
        TsSeries<Double> s = series(new double[][] {{0, 5.0}, {30, 6.0}});
        assertPairs(Fill.zero(s, 10),
                new double[][] {{0, 5.0}, {10, 0.0}, {20, 0.0}, {30, 6.0}});
    }

    @Test
    void noGapPassthrough() {
        TsSeries<Double> s = series(new double[][] {{0, 1.0}, {10, 2.0}, {20, 3.0}});
        assertPairs(Fill.linear(s, 10),
                new double[][] {{0, 1.0}, {10, 2.0}, {20, 3.0}});
    }

    @Test
    void emptyAndSingle() {
        TsSeries<Double> e = new TsSeries<>();
        assertTrue(Fill.linear(e, 10).isEmpty());
        TsSeries<Double> one = series(new double[][] {{5, 5.0}});
        assertPairs(Fill.linear(one, 10), new double[][] {{5, 5.0}});
    }

    @Test
    void stepZeroNoFill() {
        TsSeries<Double> s = series(new double[][] {{0, 0.0}, {100, 1.0}});
        assertPairs(Fill.linear(s, 0), new double[][] {{0, 0.0}, {100, 1.0}});
    }

    @Test
    void hugeStepNoFill() {
        TsSeries<Double> s = series(new double[][] {{0, 0.0}, {100, 1.0}});
        assertPairs(Fill.linear(s, 1_000), new double[][] {{0, 0.0}, {100, 1.0}});
    }

    @Test
    void partialStepRemainder() {
        TsSeries<Double> s = series(new double[][] {{0, 0.0}, {25, 25.0}});
        List<long[]> p = pairsTs(Fill.linear(s, 10));
        assertEquals(4, p.size());
        assertEquals(10, p.get(1)[0]);
        assertEquals(20, p.get(2)[0]);
        assertEquals(25, p.get(3)[0]);
    }

    @Test
    void multipleGaps() {
        TsSeries<Double> s = series(new double[][] {{0, 0.0}, {20, 2.0}, {21, 9.0}, {60, 6.0}});
        List<long[]> p = pairsTs(Fill.linear(s, 10));
        long[] expect = {0, 10, 20, 21, 31, 41, 51, 60};
        assertEquals(expect.length, p.size());
        for (int i = 0; i < expect.length; i++) {
            assertEquals(expect[i], p.get(i)[0], "ts at " + i);
        }
    }

    @Test
    void outputStrictlyIncreasing() {
        TsSeries<Double> s = series(new double[][] {{0, 0.0}, {37, 3.7}, {90, 9.0}});
        List<long[]> p = pairsTs(Fill.linear(s, 10));
        for (int i = 1; i < p.size(); i++) {
            assertTrue(p.get(i)[0] > p.get(i - 1)[0], "ts must strictly increase");
        }
    }

    @Test
    void locfMultipleGaps() {
        TsSeries<Double> s = series(new double[][] {{0, 3.0}, {35, 9.0}});
        assertPairs(Fill.locf(s, 10),
                new double[][] {{0, 3.0}, {10, 3.0}, {20, 3.0}, {30, 3.0}, {35, 9.0}});
    }
}
