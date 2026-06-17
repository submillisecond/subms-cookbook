package com.submillisecond.recipes.tssql;

/**
 * What can go wrong running a query. A malformed query is a {@link Kind#PARSE};
 * a clause outside the subset is {@link Kind#UNSUPPORTED} (naming the clause);
 * an unknown table / column is the matching kind; a type / shape error the
 * lowerer cannot reconcile is {@link Kind#TYPE}. Mirrors the Rust sibling's
 * {@code TsSqlError} enum (one exception with a kind tag rather than a class
 * hierarchy, matching the {@code subms-ts-promql} choice).
 */
public final class TsSqlException extends RuntimeException {

    public enum Kind {
        PARSE,
        UNKNOWN_TABLE,
        UNKNOWN_COLUMN,
        UNSUPPORTED,
        TYPE
    }

    private final Kind kind;

    private TsSqlException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsSqlException parse(String message) {
        return new TsSqlException(Kind.PARSE, "sql parse error: " + message);
    }

    public static TsSqlException unknownTable(String table) {
        return new TsSqlException(Kind.UNKNOWN_TABLE, "unknown table: " + table);
    }

    public static TsSqlException unknownColumn(String column) {
        return new TsSqlException(Kind.UNKNOWN_COLUMN, "unknown column: " + column);
    }

    public static TsSqlException unsupported(String clause) {
        return new TsSqlException(Kind.UNSUPPORTED, "unsupported clause: " + clause);
    }

    public static TsSqlException type(String message) {
        return new TsSqlException(Kind.TYPE, "type error: " + message);
    }
}
