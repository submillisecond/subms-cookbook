package com.submillisecond.recipes.tsasofjoin;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * As-of joins over two time series. For each point in the left series, find
 * the matching point in the right by timestamp: backward (largest right ts
 * {@code <=} left ts), forward (smallest right ts {@code >=} left ts), or
 * nearest within a tolerance. The join every market-data / sensor-fusion
 * pipeline needs ("what was the bid when this trade printed?").
 *
 * <p>Backward and forward run as a single linear merge-walk over both series
 * ({@code O(n + m)}, no per-point search); nearest does a bounded two-sided
 * lookup. Both series are assumed ts-ordered (the {@link TsSeries} contract).
 *
 * <pre>
 *   TsSeries&lt;Double&gt; trades = new TsSeries&lt;&gt;();
 *   trades.push(10, 100.0);
 *   trades.push(25, 101.0);
 *   TsSeries&lt;Double&gt; quotes = new TsSeries&lt;&gt;();
 *   quotes.push(5, 99.5);
 *   quotes.push(20, 99.8);
 *
 *   List&lt;TsMatch&gt; rows = AsofJoin.backward(trades, quotes);
 *   rows.get(0).right().get().ts(); // 5  - quote as-of trade@10
 *   rows.get(1).right().get().ts(); // 20 - quote as-of trade@25
 * </pre>
 */
public final class AsofJoin {

    private AsofJoin() {}

    /**
     * One joined row: a left point and its matched right point (if any).
     * {@code right} is empty when nothing qualifies, so the join never
     * silently drops a left row.
     */
    public record TsMatch(TsPoint<Double> left, Optional<TsPoint<Double>> right) {}

    /** For each left point, the right point with the largest ts {@code <=} left.ts. */
    public static List<TsMatch> backward(TsSeries<Double> left, TsSeries<Double> right) {
        List<TsPoint<Double>> r = collect(right);
        List<TsMatch> out = new ArrayList<>(left.size());
        int j = 0;
        TsPoint<Double> last = null;
        for (TsPoint<Double> lp : left) {
            while (j < r.size() && r.get(j).ts() <= lp.ts()) {
                last = r.get(j);
                j++;
            }
            out.add(new TsMatch(lp, Optional.ofNullable(last)));
        }
        return out;
    }

    /** For each left point, the right point with the smallest ts {@code >=} left.ts. */
    public static List<TsMatch> forward(TsSeries<Double> left, TsSeries<Double> right) {
        List<TsPoint<Double>> r = collect(right);
        List<TsMatch> out = new ArrayList<>(left.size());
        int j = 0;
        for (TsPoint<Double> lp : left) {
            while (j < r.size() && r.get(j).ts() < lp.ts()) {
                j++;
            }
            Optional<TsPoint<Double>> match = j < r.size() ? Optional.of(r.get(j)) : Optional.empty();
            out.add(new TsMatch(lp, match));
        }
        return out;
    }

    /**
     * For each left point, the nearest right point by absolute ts distance,
     * only if within {@code toleranceNs} (else empty). Ties resolve to the
     * earlier point.
     */
    public static List<TsMatch> nearest(TsSeries<Double> left, TsSeries<Double> right, long toleranceNs) {
        List<TsPoint<Double>> r = collect(right);
        List<TsMatch> out = new ArrayList<>(left.size());
        int j = 0;
        boolean haveBack = false;
        for (TsPoint<Double> lp : left) {
            while (j < r.size() && r.get(j).ts() <= lp.ts()) {
                haveBack = true;
                j++;
            }
            TsPoint<Double> back = haveBack ? r.get(j - 1) : null;
            TsPoint<Double> fwd = j < r.size() ? r.get(j) : null;
            TsPoint<Double> pick;
            if (back != null && fwd != null) {
                pick = (lp.ts() - back.ts()) <= (fwd.ts() - lp.ts()) ? back : fwd;
            } else if (back != null) {
                pick = back;
            } else {
                pick = fwd;
            }
            Optional<TsPoint<Double>> within = Optional.empty();
            if (pick != null && Math.abs(pick.ts() - lp.ts()) <= toleranceNs) {
                within = Optional.of(pick);
            }
            out.add(new TsMatch(lp, within));
        }
        return out;
    }

    private static List<TsPoint<Double>> collect(TsSeries<Double> series) {
        List<TsPoint<Double>> out = new ArrayList<>(series.size());
        for (TsPoint<Double> p : series) {
            out.add(p);
        }
        return out;
    }
}
