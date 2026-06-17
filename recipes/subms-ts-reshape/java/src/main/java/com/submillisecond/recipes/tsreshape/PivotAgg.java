package com.submillisecond.recipes.tsreshape;

/**
 * How {@link TsReshape#pivot} collapses the value cells that fall into a single
 * (index, column) bucket. {@code LAST} keeps the value from the last source row
 * in input order; the rest are the obvious reductions over the bucket's valid
 * values.
 */
public enum PivotAgg {
    SUM,
    MEAN,
    MIN,
    MAX,
    LAST
}
