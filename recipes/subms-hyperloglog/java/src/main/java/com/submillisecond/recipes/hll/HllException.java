package com.submillisecond.recipes.hll;

/**
 * Typed failure surface, the Java mirror of the Rust {@code HllError} enum.
 *
 * <p>Merge and the wire codec both fail for reasons a caller can act on - a
 * precision mismatch is a config bug, a truncated buffer is a transport bug -
 * so the failure names itself. Extends {@link IllegalArgumentException} so
 * existing catch blocks keep working.
 */
public final class HllException extends IllegalArgumentException {

    private static final long serialVersionUID = 1L;

    /** What went wrong. One constant per Rust {@code HllError} variant. */
    public enum Kind {
        PRECISION_MISMATCH,
        INVALID_PRECISION,
        BAD_MAGIC,
        UNSUPPORTED_VERSION,
        UNSUPPORTED_ENCODING,
        TRUNCATED
    }

    private final Kind kind;

    private HllException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static HllException precisionMismatch(int left, int right) {
        return new HllException(Kind.PRECISION_MISMATCH,
                "precision mismatch: " + left + " vs " + right);
    }

    public static HllException invalidPrecision(int p) {
        return new HllException(Kind.INVALID_PRECISION, "precision " + p + " outside [4, 18]");
    }

    public static HllException badMagic() {
        return new HllException(Kind.BAD_MAGIC, "bad magic: not a subms-hyperloglog buffer");
    }

    public static HllException unsupportedVersion(int v) {
        return new HllException(Kind.UNSUPPORTED_VERSION, "unsupported format version " + v);
    }

    public static HllException unsupportedEncoding(int e) {
        return new HllException(Kind.UNSUPPORTED_ENCODING, "unsupported encoding " + e);
    }

    public static HllException truncated(int expected, int actual) {
        return new HllException(Kind.TRUNCATED,
                "truncated buffer: expected " + expected + " bytes, got " + actual);
    }
}
