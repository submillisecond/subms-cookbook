package com.submillisecond.recipes.tdigest;

/**
 * Raised when a byte buffer cannot be decoded as a t-digest: an unknown
 * version byte, or a buffer that ends before the declared centroid count is
 * read.
 */
public final class TsTDigestException extends RuntimeException {

    public enum Kind {
        BAD_VERSION,
        TRUNCATED
    }

    private final Kind kind;
    private final int version;

    private TsTDigestException(Kind kind, int version, String message) {
        super(message);
        this.kind = kind;
        this.version = version;
    }

    static TsTDigestException badVersion(int version) {
        return new TsTDigestException(Kind.BAD_VERSION, version, "unknown t-digest version " + version);
    }

    static TsTDigestException truncated() {
        return new TsTDigestException(Kind.TRUNCATED, -1, "truncated t-digest bytes");
    }

    public Kind kind() {
        return kind;
    }

    /** The offending version byte for {@link Kind#BAD_VERSION}, else -1. */
    public int version() {
        return version;
    }
}
