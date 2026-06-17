package com.submillisecond.recipes.tscbor;

/**
 * Failure decoding a CBOR buffer that is not a well-formed series image.
 * Mirrors the Rust {@code TsCborError} enum: {@link Kind#TRUNCATED} when the
 * buffer ends mid-token, {@link Kind#UNEXPECTED} when a head's major type or
 * value does not match the grammar.
 */
public final class TsCborException extends RuntimeException {

    public enum Kind {
        TRUNCATED,
        UNEXPECTED
    }

    private final Kind kind;

    private TsCborException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsCborException truncated() {
        return new TsCborException(Kind.TRUNCATED, "truncated CBOR buffer");
    }

    public static TsCborException unexpected(String detail) {
        return new TsCborException(Kind.UNEXPECTED, "unexpected CBOR token: " + detail);
    }
}
