package com.submillisecond.recipes.tscardinality;

/**
 * A tenant handle. A thin value type so a tenant id never gets confused with a
 * series id or a count.
 */
public record TsTenantId(long value) {
    public static TsTenantId of(long value) {
        return new TsTenantId(value);
    }
}
