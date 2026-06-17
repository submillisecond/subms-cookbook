package com.submillisecond.recipes.tsgzip;

/**
 * Failure decoding a gzip stream. Mirrors the Rust {@code TsGzipError} enum: a
 * malformed container ({@link Kind#TRUNCATED}, {@link Kind#BAD_MAGIC},
 * {@link Kind#BAD_METHOD}, {@link Kind#UNSUPPORTED_FLAG}), a DEFLATE body that
 * fails to inflate ({@link Kind#INFLATE}), or a trailer that does not match the
 * inflated bytes ({@link Kind#CRC_MISMATCH}, {@link Kind#SIZE_MISMATCH}).
 */
public final class TsGzipException extends RuntimeException {

    public enum Kind {
        TRUNCATED,
        BAD_MAGIC,
        BAD_METHOD,
        UNSUPPORTED_FLAG,
        INFLATE,
        CRC_MISMATCH,
        SIZE_MISMATCH
    }

    private final transient Kind kind;

    private TsGzipException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsGzipException truncated() {
        return new TsGzipException(Kind.TRUNCATED, "truncated gzip stream");
    }

    public static TsGzipException badMagic() {
        return new TsGzipException(Kind.BAD_MAGIC, "bad gzip magic (expected 1f 8b)");
    }

    public static TsGzipException badMethod(int m) {
        return new TsGzipException(Kind.BAD_METHOD, "unsupported gzip method " + m + " (expected 8)");
    }

    public static TsGzipException unsupportedFlag(int flg) {
        return new TsGzipException(Kind.UNSUPPORTED_FLAG,
                "unsupported gzip flag bits 0x" + Integer.toHexString(flg));
    }

    public static TsGzipException inflate(String detail) {
        return new TsGzipException(Kind.INFLATE, "inflate failed: " + detail);
    }

    public static TsGzipException crcMismatch(long expected, long actual) {
        return new TsGzipException(Kind.CRC_MISMATCH,
                "gzip CRC mismatch: expected 0x" + Long.toHexString(expected)
                        + ", got 0x" + Long.toHexString(actual));
    }

    public static TsGzipException sizeMismatch(long expected, long actual) {
        return new TsGzipException(Kind.SIZE_MISMATCH,
                "gzip ISIZE mismatch: expected " + expected + ", got " + actual);
    }
}
