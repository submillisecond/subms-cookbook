package com.submillisecond.recipes.ts;

public final class TsCodecException extends RuntimeException {

    public enum Kind {
        PARSE,
        UNSUPPORTED_TIMESTAMP_DECODE
    }

    private final Kind kind;

    private TsCodecException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    static TsCodecException parse(String message) {
        return new TsCodecException(Kind.PARSE, "parse error: " + message);
    }

    static TsCodecException unsupportedTimestampDecode() {
        return new TsCodecException(Kind.UNSUPPORTED_TIMESTAMP_DECODE,
                "decoding ISO-8601 timestamps requires the datetime surface");
    }

    public Kind kind() {
        return kind;
    }
}
