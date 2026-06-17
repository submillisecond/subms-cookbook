package com.submillisecond.recipes.tsarrow;

/** Error surface for the Arrow adapter. */
public final class TsArrowException extends RuntimeException {

    public enum Kind {
        MAPPING,
        ARROW,
        IPC
    }

    private final Kind kind;

    private TsArrowException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsArrowException mapping(String message) {
        return new TsArrowException(Kind.MAPPING, message);
    }

    public static TsArrowException arrow(String message) {
        return new TsArrowException(Kind.ARROW, message);
    }

    public static TsArrowException ipc(String message) {
        return new TsArrowException(Kind.IPC, message);
    }
}
