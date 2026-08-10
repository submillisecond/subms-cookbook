package com.submillisecond.recipes.cuckoo;

/**
 * Every way an operation on a {@link CuckooFilter} can refuse. Mirrors the
 * Rust port's {@code CuckooError} enum one-for-one.
 */
public final class CuckooException extends RuntimeException {

    private static final long serialVersionUID = 1L;

    public enum Reason {
        /**
         * The eviction chain hit {@link CuckooFilter#MAX_KICKS} and the victim
         * slot was already occupied, so there is nowhere left to put a
         * fingerprint. Size the filter larger, or reach for
         * {@code DynamicCuckooFilter}.
         */
        NOT_ENOUGH_SPACE,
        /**
         * {@link CuckooFilter#union} was handed a filter with a different
         * bucket count. Bucket {@code i} of one filter has no relationship to
         * bucket {@code i} of the other unless the geometries match.
         */
        GEOMETRY_MISMATCH
    }

    private final Reason reason;

    public CuckooException(Reason reason, String message) {
        super(message);
        this.reason = reason;
    }

    public Reason reason() { return reason; }
}
