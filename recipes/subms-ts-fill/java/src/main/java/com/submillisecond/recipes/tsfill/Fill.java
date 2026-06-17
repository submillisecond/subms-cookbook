package com.submillisecond.recipes.tsfill;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Gap fill for irregular time series. Where consecutive points are more than
 * {@code stepNs} apart, insert synthetic points every {@code stepNs} between
 * them, filled by one of three policies: linear interpolation,
 * last-observation-carried-forward (LOCF), or zero. Original points are always
 * preserved and the output is strictly increasing in ts.
 *
 * <pre>
 *   TsSeries&lt;Double&gt; s = new TsSeries&lt;&gt;();
 *   s.push(0, 0.0);
 *   s.push(40, 4.0);                  // a 40-wide gap
 *   TsSeries&lt;Double&gt; filled = Fill.linear(s, 10); // insert at 10, 20, 30
 *   // -&gt; (0,0) (10,1) (20,2) (30,3) (40,4)
 * </pre>
 */
public final class Fill {

    private Fill() {}

    @FunctionalInterface
    private interface ValueAt {
        double apply(TsPoint<Double> a, TsPoint<Double> b, double frac);
    }

    /** Linear interpolation between the bracketing points. */
    public static TsSeries<Double> linear(TsSeries<Double> series, long stepNs) {
        return fillWith(series, stepNs, (a, b, frac) -> a.value() + frac * (b.value() - a.value()));
    }

    /** Last observation carried forward: each gap point repeats the left value. */
    public static TsSeries<Double> locf(TsSeries<Double> series, long stepNs) {
        return fillWith(series, stepNs, (a, b, frac) -> a.value());
    }

    /** Zero fill: each gap point is 0.0 (e.g. a counter with no events). */
    public static TsSeries<Double> zero(TsSeries<Double> series, long stepNs) {
        return fillWith(series, stepNs, (a, b, frac) -> 0.0);
    }

    private static TsSeries<Double> fillWith(TsSeries<Double> series, long stepNs, ValueAt valueAt) {
        List<TsPoint<Double>> pts = new ArrayList<>(series.size());
        for (TsPoint<Double> p : series) {
            pts.add(p);
        }
        TsSeries<Double> out = TsSeries.withCapacity(pts.size());
        if (pts.isEmpty()) {
            return out;
        }
        out.push(pts.get(0).ts(), pts.get(0).value());
        for (int i = 1; i < pts.size(); i++) {
            TsPoint<Double> a = pts.get(i - 1);
            TsPoint<Double> b = pts.get(i);
            long gap = b.ts() - a.ts();
            if (stepNs > 0 && gap > stepNs) {
                long t = a.ts() + stepNs;
                while (t < b.ts()) {
                    double frac = (double) (t - a.ts()) / (double) gap;
                    out.push(t, valueAt.apply(a, b, frac));
                    t += stepNs;
                }
            }
            out.push(b.ts(), b.value());
        }
        return out;
    }
}
