package com.submillisecond.recipes.tswal;

/**
 * Wraps an {@link java.io.IOException} from a log operation, plus a corrupt-state
 * signal for malformed segment-directory layout. The replay path treats a torn
 * tail as a clean stop (not an exception), so {@link Kind#CORRUPT} is reserved
 * for genuinely unreadable directory state.
 */
public final class TsWalException extends RuntimeException {

    /** Whether the failure was an IO error or a structural corruption. */
    public enum Kind {
        IO,
        CORRUPT
    }

    private final Kind kind;

    public TsWalException(Kind kind, String message, Throwable cause) {
        super(message, cause);
        this.kind = kind;
    }

    public TsWalException(Kind kind, String message) {
        this(kind, message, null);
    }

    public Kind kind() {
        return kind;
    }
}
