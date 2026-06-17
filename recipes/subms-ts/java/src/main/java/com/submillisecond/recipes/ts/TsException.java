package com.submillisecond.recipes.ts;

/**
 * Thrown by the ingest + range surface. Time-query and aggregate reads never
 * throw - they return {@code null} / {@link java.util.Optional} for the empty
 * case. Mirrors the Rust {@code TsError} enum: a {@link Kind} discriminator
 * plus the offending values.
 */
public final class TsException extends RuntimeException {

    /** Which ingest rule the offending value broke. */
    public enum Kind {
        /** A timestamp earlier than the series tail. */
        NOT_MONOTONIC,
        /** A null / non-finite observation. */
        NULL_VALUE
    }

    private final Kind kind;
    private final long last;
    private final long got;

    private TsException(Kind kind, long last, long got, String message) {
        super(message);
        this.kind = kind;
        this.last = last;
        this.got = got;
    }

    static TsException notMonotonic(long last, long got) {
        return new TsException(Kind.NOT_MONOTONIC, last, got,
                "non-monotonic ts: tail=" + last + ", got=" + got);
    }

    static TsException nullValue(String hint) {
        return new TsException(Kind.NULL_VALUE, 0L, 0L, "null value rejected: " + hint);
    }

    public Kind kind() {
        return kind;
    }

    /** Series tail at the time of a {@link Kind#NOT_MONOTONIC} rejection. */
    public long last() {
        return last;
    }

    /** Offending timestamp for a {@link Kind#NOT_MONOTONIC} rejection. */
    public long got() {
        return got;
    }
}
