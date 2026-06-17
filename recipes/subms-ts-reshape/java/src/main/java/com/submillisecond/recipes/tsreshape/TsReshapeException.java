package com.submillisecond.recipes.tsreshape;

import java.util.List;

/**
 * Errors the reshape surface can raise. All are caller-input errors caught up
 * front; a successful call never partially fails. Mirrors the Rust
 * {@code TsReshapeError} variants.
 */
public final class TsReshapeException extends RuntimeException {

    private TsReshapeException(String message) {
        super(message);
    }

    /** A named column is not present in the frame it was requested from. */
    public static TsReshapeException unknownColumn(String name) {
        return new TsReshapeException("unknown column: " + name);
    }

    /** A reshape that needs at least one column got an empty column list. */
    public static TsReshapeException noColumns() {
        return new TsReshapeException("reshape needs at least one column");
    }

    /** vstack / set-op schema mismatch: differing slot names or order. */
    public static TsReshapeException schemaMismatch(List<String> a, List<String> b) {
        return new TsReshapeException("schema mismatch: a=" + a + ", b=" + b);
    }

    /** hstack row-count mismatch: the two frames do not share a row axis. */
    public static TsReshapeException rowCountMismatch(int a, int b) {
        return new TsReshapeException("hstack row-count mismatch: a=" + a + ", b=" + b);
    }
}
