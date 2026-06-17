package com.submillisecond.recipes.tsresample;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Snap an irregular series onto a regular time grid. Points are grouped into
 * fixed-width buckets {@code [k*period, (k+1)*period)} and each bucket
 * collapses to one value per the chosen {@link TsResampleMode}, emitted at the
 * bucket start. The step that turns ragged event data into the evenly-spaced
 * series a chart axis or a model expects.
 *
 * <pre>
 *   TsSeries&lt;Double&gt; s = new TsSeries&lt;&gt;();
 *   s.push(0, 1.0);
 *   s.push(3, 3.0);    // bucket [0,10)
 *   s.push(11, 5.0);   // bucket [10,20)
 *   TsSeries&lt;Double&gt; g = Resample.toGrid(s, 10, TsResampleMode.MEAN);
 *   // (0, 2.0), (10, 5.0)
 * </pre>
 */
public final class Resample {

    private Resample() {
    }

    private static final class Bucket {
        final long start;
        long count;
        double sum;
        final double first;
        double last;
        double min;
        double max;

        Bucket(long start, double value) {
            this.start = start;
            this.count = 1;
            this.sum = value;
            this.first = value;
            this.last = value;
            this.min = value;
            this.max = value;
        }

        void update(double value) {
            count++;
            sum += value;
            last = value;
            if (value < min) min = value;
            if (value > max) max = value;
        }

        double value(TsResampleMode mode) {
            return switch (mode) {
                case MEAN -> sum / (double) count;
                case LAST -> last;
                case FIRST -> first;
                case SUM -> sum;
                case COUNT -> (double) count;
                case MIN -> min;
                case MAX -> max;
            };
        }
    }

    /**
     * Resample {@code series} onto a {@code periodNs} grid using {@code mode}.
     * Empty buckets (no points) are not emitted - the output is as sparse as
     * the input's bucket coverage. {@code periodNs <= 0} returns an empty
     * series.
     */
    public static TsSeries<Double> toGrid(TsSeries<Double> series, long periodNs, TsResampleMode mode) {
        TsSeries<Double> out = new TsSeries<>();
        if (periodNs <= 0) {
            return out;
        }
        Bucket cur = null;
        for (TsPoint<Double> p : series) {
            long start = Math.floorDiv(p.ts(), periodNs) * periodNs;
            double value = p.value();
            if (cur != null && cur.start == start) {
                cur.update(value);
            } else {
                if (cur != null) {
                    out.push(cur.start, cur.value(mode));
                }
                cur = new Bucket(start, value);
            }
        }
        if (cur != null) {
            out.push(cur.start, cur.value(mode));
        }
        return out;
    }
}
