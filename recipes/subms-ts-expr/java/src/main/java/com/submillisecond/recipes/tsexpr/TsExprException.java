package com.submillisecond.recipes.tsexpr;

/**
 * Raised by the evaluator for pure-compute errors only. A structurally missing
 * cell is a validity bit, not an exception. Mirrors the Rust sibling's
 * {@code TsExprError} (UnknownColumn / TypeMismatch / NotScalar).
 */
public final class TsExprException extends RuntimeException {

    public enum Kind {
        UNKNOWN_COLUMN,
        TYPE_MISMATCH,
        NOT_SCALAR
    }

    private final Kind kind;

    private TsExprException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsExprException unknownColumn(String name) {
        return new TsExprException(Kind.UNKNOWN_COLUMN, "unknown column: " + name);
    }

    public static TsExprException typeMismatch(String why) {
        return new TsExprException(Kind.TYPE_MISMATCH, "type mismatch: " + why);
    }

    public static TsExprException notScalar() {
        return new TsExprException(
                Kind.NOT_SCALAR, "evalScalar requires a top-level Agg expression");
    }
}
