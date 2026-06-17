package com.submillisecond.recipes.tsjoin;

/**
 * The six equi-join kinds. {@link TsJoin#hashJoin} and
 * {@link TsJoin#sortMergeJoin} implement all six and agree on the result set;
 * {@link TsJoin#crossJoin} ignores the kind.
 */
public enum TsJoinKind {
    /** Rows whose keys match on both sides. Left columns + right columns. */
    INNER,
    /** Every left row; right columns are missing where no match. */
    LEFT,
    /** Every right row; left columns are missing where no match. */
    RIGHT,
    /**
     * Every matched pair, plus unmatched-left (right missing) and
     * unmatched-right (left missing).
     */
    OUTER,
    /**
     * Left rows that have at least one right match. Left columns only; each
     * qualifying left row appears once regardless of match multiplicity.
     */
    SEMI,
    /** Left rows that have no right match. Left columns only. */
    ANTI
}
