package com.submillisecond.recipes.tsexpr;

/** Reductions over the valid cells of a column; each broadcasts its scalar. */
public enum TsAggOp {
    SUM,
    MIN,
    MAX,
    MEAN,
    COUNT
}
