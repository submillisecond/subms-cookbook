package com.submillisecond.recipes.ts;

/**
 * API-level marker for series that expose the scalar numeric aggregate
 * surface on a primitive column. Implemented by {@link TsSeriesD} and
 * {@link TsSeriesL}. Storage is deliberately NOT inherited from
 * {@link TsSeries} - the unboxed series keep their own primitive columns to
 * avoid re-boxing on the hot path.
 */
public interface TsNumericSeries {

    int size();

    boolean isEmpty();

    double meanOrNaN();
}
