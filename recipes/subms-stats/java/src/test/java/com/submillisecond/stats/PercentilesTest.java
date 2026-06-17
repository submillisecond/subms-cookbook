package com.submillisecond.stats;

import org.junit.jupiter.api.Test;

import java.util.Arrays;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class PercentilesTest {

    @Test
    void percentileEmptyIsZero() {
        assertEquals(0L, Percentiles.percentile(new long[0], 0.5));
    }

    @Test
    void percentileKnownDistribution() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        Arrays.sort(v);
        assertEquals(50L, Percentiles.percentile(v, 0.50));
        assertEquals(99L, Percentiles.percentile(v, 0.99));
        assertEquals(99L, Percentiles.percentile(v, 1.0));
    }

    @Test
    void percentileSweepEndpointsIncluded() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        List<double[]> sweep = Percentiles.percentileSweep(v, 0.0, 1.0, 0.5);
        assertEquals(3, sweep.size());
        assertEquals(0.0, sweep.get(0)[0]);
        assertEquals(1.0, sweep.get(2)[0]);
    }

    @Test
    void percentileSweepRejectsZeroStep() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        assertTrue(Percentiles.percentileSweep(v, 0.0, 1.0, 0.0).isEmpty());
    }

    @Test
    void meanStddevBasic() {
        long[] samples = {100L, 200L, 300L, 400L};
        assertEquals(250L, Percentiles.mean(samples));
        long sd = Percentiles.stddev(samples);
        assertTrue(sd >= 125 && sd <= 135, "stddev around 129: " + sd);
    }

    @Test
    void meanEmptyZero() {
        assertEquals(0L, Percentiles.mean(new long[0]));
    }

    @Test
    void stddevSingleSampleZero() {
        assertEquals(0L, Percentiles.stddev(new long[]{42L}));
    }

    @Test
    void percentileSweepBasicRange() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        List<double[]> sweep = Percentiles.percentileSweep(v, 0.5, 0.9, 0.1);
        // 0.5, 0.6, 0.7, 0.8, 0.9 - five quantiles.
        assertEquals(5, sweep.size());
        for (int i = 0; i < sweep.size(); i++) {
            assertTrue(sweep.get(i)[1] >= sweep.get(Math.max(0, i - 1))[1], "monotonic non-decreasing");
        }
    }
}
