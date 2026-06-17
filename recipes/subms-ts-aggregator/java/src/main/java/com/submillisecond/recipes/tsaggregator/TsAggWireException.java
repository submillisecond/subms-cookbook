package com.submillisecond.recipes.tsaggregator;

/**
 * Raised when a byte buffer cannot be decoded as a {@link TsWindowedAggregator}
 * partial-window wire image: an unknown version byte, or a buffer that ends
 * before the declared point count is read.
 */
public final class TsAggWireException extends RuntimeException {

    public enum Kind {
        BAD_VERSION,
        TRUNCATED
    }

    private final Kind kind;
    private final int version;

    private TsAggWireException(Kind kind, int version, String message) {
        super(message);
        this.kind = kind;
        this.version = version;
    }

    static TsAggWireException badVersion(int version) {
        return new TsAggWireException(Kind.BAD_VERSION, version, "unknown aggregator wire version " + version);
    }

    static TsAggWireException truncated() {
        return new TsAggWireException(Kind.TRUNCATED, -1, "truncated aggregator wire buffer");
    }

    public Kind kind() {
        return kind;
    }

    /** The offending version byte for {@link Kind#BAD_VERSION}, else -1. */
    public int version() {
        return version;
    }
}
