package com.submillisecond.recipes.tsyaml;

/**
 * Failure decoding a YAML buffer that is not a well-formed series document.
 * Mirrors the Rust {@code TsYamlError} enum: {@link Kind#PARSE} for malformed
 * YAML or a shape the grammar does not expect, and
 * {@link Kind#UNSUPPORTED_TIMESTAMP_DECODE} for the {@code ISO8601} style, which
 * is encode-only until the {@code datetime} surface lands.
 */
public final class TsYamlException extends RuntimeException {

    public enum Kind {
        PARSE,
        UNSUPPORTED_TIMESTAMP_DECODE
    }

    private final Kind kind;

    private TsYamlException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsYamlException parse(String detail) {
        return new TsYamlException(Kind.PARSE, "yaml parse error: " + detail);
    }

    public static TsYamlException unsupportedTimestampDecode() {
        return new TsYamlException(Kind.UNSUPPORTED_TIMESTAMP_DECODE,
                "decoding ISO-8601 timestamps requires the datetime surface");
    }
}
