package com.submillisecond.recipes.tsresample;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

class ResampleTest {

    private static TsSeries<Double> series(long[][] pts) {
        TsSeries<Double> s = new TsSeries<>();
        for (long[] p : pts) {
            s.push(p[0], (double) p[1]);
        }
        return s;
    }

    private static List<long[]> grid(TsSeries<Double> s, long period, TsResampleMode mode) {
        List<long[]> out = new ArrayList<>();
        for (TsPoint<Double> p : Resample.toGrid(s, period, mode)) {
            out.add(new long[] {p.ts(), Math.round(p.value() * 1000.0)});
        }
        return out;
    }

    private static void assertGrid(List<long[]> got, long[][] want) {
        assertEquals(want.length, got.size(), "row count");
        for (int i = 0; i < want.length; i++) {
            assertEquals(want[i][0], got.get(i)[0], "ts at row " + i);
            assertEquals(want[i][1] * 1000, got.get(i)[1], "value at row " + i);
        }
    }

    @Test
    void meanBuckets() {
        TsSeries<Double> s = series(new long[][] {{0, 1}, {3, 3}, {11, 5}, {19, 7}});
        assertGrid(grid(s, 10, TsResampleMode.MEAN), new long[][] {{0, 2}, {10, 6}});
    }

    @Test
    void lastAndFirst() {
        TsSeries<Double> s = series(new long[][] {{0, 1}, {3, 3}, {9, 9}, {11, 5}});
        assertGrid(grid(s, 10, TsResampleMode.LAST), new long[][] {{0, 9}, {10, 5}});
        assertGrid(grid(s, 10, TsResampleMode.FIRST), new long[][] {{0, 1}, {10, 5}});
    }

    @Test
    void sumAndCount() {
        TsSeries<Double> s = series(new long[][] {{0, 1}, {3, 2}, {5, 3}, {11, 4}});
        assertGrid(grid(s, 10, TsResampleMode.SUM), new long[][] {{0, 6}, {10, 4}});
        assertGrid(grid(s, 10, TsResampleMode.COUNT), new long[][] {{0, 3}, {10, 1}});
    }

    @Test
    void minAndMax() {
        TsSeries<Double> s = series(new long[][] {{0, 5}, {3, 1}, {9, 9}, {11, 7}});
        assertGrid(grid(s, 10, TsResampleMode.MIN), new long[][] {{0, 1}, {10, 7}});
        assertGrid(grid(s, 10, TsResampleMode.MAX), new long[][] {{0, 9}, {10, 7}});
    }

    @Test
    void emptySeries() {
        TsSeries<Double> s = new TsSeries<>();
        assertTrue(Resample.toGrid(s, 10, TsResampleMode.MEAN).isEmpty());
    }

    @Test
    void singlePoint() {
        TsSeries<Double> s = series(new long[][] {{7, 42}});
        assertGrid(grid(s, 10, TsResampleMode.MEAN), new long[][] {{0, 42}});
    }

    @Test
    void periodZeroEmpty() {
        TsSeries<Double> s = series(new long[][] {{0, 1}, {5, 2}});
        assertTrue(Resample.toGrid(s, 0, TsResampleMode.MEAN).isEmpty());
        assertTrue(Resample.toGrid(s, -10, TsResampleMode.MEAN).isEmpty());
    }

    @Test
    void sparseBucketsNoEmpties() {
        TsSeries<Double> s = series(new long[][] {{0, 1}, {5, 2}, {55, 9}});
        List<long[]> g = grid(s, 10, TsResampleMode.MEAN);
        assertEquals(2, g.size());
        assertEquals(0, g.get(0)[0]);
        assertEquals(1500, g.get(0)[1]); // bucket [0,10) mean = 1.5
        assertEquals(50, g.get(1)[0]);
        assertEquals(9000, g.get(1)[1]);
    }

    @Test
    void bucketAlignmentAbsolute() {
        TsSeries<Double> s = series(new long[][] {{7, 1}, {13, 2}});
        assertGrid(grid(s, 10, TsResampleMode.FIRST), new long[][] {{0, 1}, {10, 2}});
    }

    @Test
    void negativeTsBuckets() {
        TsSeries<Double> s = series(new long[][] {{-15, 1}, {-12, 2}, {-5, 3}});
        // -15,-12 -> [-20,-10); -5 -> [-10,0)
        List<long[]> g = grid(s, 10, TsResampleMode.MEAN);
        assertEquals(2, g.size());
        assertEquals(-20, g.get(0)[0]);
        assertEquals(1500, g.get(0)[1]);
        assertEquals(-10, g.get(1)[0]);
        assertEquals(3000, g.get(1)[1]);
    }

    @Test
    void outputStrictlyIncreasing() {
        TsSeries<Double> s = new TsSeries<>();
        for (int i = 0; i < 500; i++) {
            s.push(i * 7L, (double) i);
        }
        List<long[]> g = grid(s, 100, TsResampleMode.MEAN);
        for (int i = 1; i < g.size(); i++) {
            assertTrue(g.get(i)[0] > g.get(i - 1)[0], "strictly increasing at " + i);
        }
    }
}
