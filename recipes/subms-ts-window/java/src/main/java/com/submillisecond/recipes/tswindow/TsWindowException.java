package com.submillisecond.recipes.tswindow;

/**
 * Raised by the window engine for structural errors only - a referenced column
 * (the value column, an order-by column, or a partition key) is missing, a
 * running reduction was asked of a non-numeric column, or {@code over} was
 * handed a non-aggregation expression. A row whose key or input is null is
 * handled by the validity model, not an exception. Mirrors the Rust sibling's
 * {@code TsWindowError} (UnknownColumn / NotNumeric / NotAnAggregation).
 */
public final class TsWindowException extends RuntimeException {

    public enum Kind {
        UNKNOWN_COLUMN,
        NOT_NUMERIC,
        NOT_AN_AGGREGATION
    }

    private final Kind kind;

    private TsWindowException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsWindowException unknownColumn(String name) {
        return new TsWindowException(Kind.UNKNOWN_COLUMN, "unknown column: " + name);
    }

    public static TsWindowException notNumeric(String name) {
        return new TsWindowException(
                Kind.NOT_NUMERIC, "column '" + name + "' is not numeric (F64/I64)");
    }

    public static TsWindowException notAnAggregation() {
        return new TsWindowException(
                Kind.NOT_AN_AGGREGATION, "over requires a top-level Agg expression");
    }
}
