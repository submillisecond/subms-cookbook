package com.submillisecond.recipes.tspromql;

/**
 * What can go wrong evaluating a query. A malformed query is a
 * {@link Kind#PARSE}; a query that parses but cannot be evaluated as written is
 * a {@link Kind#EVAL}. Mirrors the Rust {@code TsPromQlError} enum.
 */
public final class TsPromQlException extends RuntimeException {

    public enum Kind {
        PARSE,
        EVAL
    }

    private final Kind kind;

    private TsPromQlException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsPromQlException parse(String message) {
        return new TsPromQlException(Kind.PARSE, "promql parse error: " + message);
    }

    public static TsPromQlException eval(String message) {
        return new TsPromQlException(Kind.EVAL, "promql eval error: " + message);
    }
}
