package com.submillisecond.recipes.tscardinality;

/**
 * Admission failure. {@link #kind()} distinguishes the global cap from a
 * per-tenant cap; {@link #max()} is the cap that bound and {@link #tenant()}
 * (for the tenanted case) which tenant tripped it.
 */
public final class TsCardinalityException extends RuntimeException {

    public enum Kind {
        CARDINALITY_CAP,
        TENANT_CARDINALITY_CAP
    }

    private final Kind kind;
    private final long tenant;
    private final int max;

    private TsCardinalityException(Kind kind, long tenant, int max, String message) {
        super(message);
        this.kind = kind;
        this.tenant = tenant;
        this.max = max;
    }

    static TsCardinalityException cardinalityCap(int max) {
        return new TsCardinalityException(
                Kind.CARDINALITY_CAP, -1L, max,
                "series cardinality cap reached (max " + max + ")");
    }

    static TsCardinalityException tenantCap(long tenant, int max) {
        return new TsCardinalityException(
                Kind.TENANT_CARDINALITY_CAP, tenant, max,
                "tenant " + tenant + " cardinality cap reached (max " + max + ")");
    }

    public Kind kind() {
        return kind;
    }

    /** The tenant that tripped the cap, or -1 for the global cap. */
    public long tenant() {
        return tenant;
    }

    public int max() {
        return max;
    }
}
