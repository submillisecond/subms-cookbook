package com.submillisecond.recipes.tsgroupby;

/**
 * Raised by the group-by surface for structural errors: an unknown column, an
 * empty key list, or an aggregation expression that is not a top-level Agg.
 * Mirrors the Rust sibling's {@code GroupByError} enum.
 */
public final class GroupByException extends RuntimeException {

    public enum Kind {
        UNKNOWN_COLUMN,
        NO_KEYS,
        NOT_AN_AGGREGATION
    }

    private final Kind kind;

    private GroupByException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static GroupByException unknownColumn(String name) {
        return new GroupByException(Kind.UNKNOWN_COLUMN, "unknown column: " + name);
    }

    public static GroupByException noKeys() {
        return new GroupByException(Kind.NO_KEYS, "group_by requires at least one key column");
    }

    public static GroupByException notAnAggregation(String name) {
        return new GroupByException(
                Kind.NOT_AN_AGGREGATION,
                "aggregation '" + name + "' is not a top-level Agg expression");
    }
}
