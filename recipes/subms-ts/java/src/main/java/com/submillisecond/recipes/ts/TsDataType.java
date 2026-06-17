package com.submillisecond.recipes.ts;

/**
 * The element type of a {@link TsColumn}. {@code VALUE} is the unstructured
 * escape hatch (a column of {@link TsValue} documents).
 */
public enum TsDataType {
    F64,
    I64,
    BOOL,
    STR,
    VALUE
}
