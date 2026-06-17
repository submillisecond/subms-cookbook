package com.submillisecond.recipes.tsjoin;

/**
 * Raised by the join surface for caller-input errors: an unknown key column, a
 * left/right key-arity mismatch, or zero keys on a keyed join. All are caught
 * up front; a successful join never partially fails. Mirrors the Rust sibling's
 * {@code TsJoinError} enum.
 */
public final class TsJoinException extends RuntimeException {

    public enum Kind {
        UNKNOWN_KEY,
        KEY_ARITY_MISMATCH,
        NO_KEYS
    }

    private final Kind kind;

    private TsJoinException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsJoinException unknownKey(String side, String name) {
        return new TsJoinException(Kind.UNKNOWN_KEY, "unknown " + side + " key column: " + name);
    }

    public static TsJoinException keyArityMismatch(int left, int right) {
        return new TsJoinException(
                Kind.KEY_ARITY_MISMATCH,
                "key arity mismatch: left=" + left + ", right=" + right);
    }

    public static TsJoinException noKeys() {
        return new TsJoinException(Kind.NO_KEYS, "a keyed join needs at least one key column");
    }
}
