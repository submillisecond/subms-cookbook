package com.submillisecond.recipes.gorillablock;

/**
 * Raised when a byte buffer cannot be decoded as a Gorilla block: an unknown
 * version byte, or a bitstream that ends before the declared point count is
 * read.
 */
public final class TsBlockException extends RuntimeException {

    public enum Kind {
        BAD_VERSION,
        TRUNCATED
    }

    private final Kind kind;
    private final int version;

    private TsBlockException(Kind kind, int version, String message) {
        super(message);
        this.kind = kind;
        this.version = version;
    }

    static TsBlockException badVersion(int version) {
        return new TsBlockException(Kind.BAD_VERSION, version, "unknown block version " + version);
    }

    static TsBlockException truncated() {
        return new TsBlockException(Kind.TRUNCATED, -1, "truncated block bitstream");
    }

    public Kind kind() {
        return kind;
    }

    /** The offending version byte for {@link Kind#BAD_VERSION}, else -1. */
    public int version() {
        return version;
    }
}
