package com.submillisecond.recipes.tslazy;

/**
 * Errors collect / certify-time execution can raise: a non-Bool filter
 * predicate or a sort on a column that is not present at that point in the
 * pipeline. The underlying expr evaluator throws its own
 * {@code TsExprException}, which propagates unchanged.
 */
public final class LazyException extends RuntimeException {

    /** The failure category. */
    public enum Kind {
        UNKNOWN_SORT_COLUMN,
        NON_BOOL_PREDICATE
    }

    private final Kind kind;

    private LazyException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static LazyException unknownSortColumn(String column) {
        return new LazyException(Kind.UNKNOWN_SORT_COLUMN, "sort_by: unknown column " + column);
    }

    public static LazyException nonBoolPredicate(Object dtype) {
        return new LazyException(
                Kind.NON_BOOL_PREDICATE, "filter predicate must be Bool, got " + dtype);
    }
}
