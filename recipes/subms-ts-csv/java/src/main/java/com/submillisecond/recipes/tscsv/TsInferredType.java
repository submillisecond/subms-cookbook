package com.submillisecond.recipes.tscsv;

/**
 * The element type the reader picks for a column. Maps onto a
 * {@code TsColumn} variant ({@code Value} is never inferred - it is the
 * caller's escape hatch).
 */
public enum TsInferredType {
    I64,
    F64,
    BOOL,
    STR
}
