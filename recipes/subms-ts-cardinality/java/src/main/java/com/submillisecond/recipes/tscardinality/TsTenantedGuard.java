package com.submillisecond.recipes.tscardinality;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Per-tenant cardinality. Each tenant gets the same {@code maxPerTenant} cap,
 * tracked independently, so a tenant at its limit never blocks another.
 */
public final class TsTenantedGuard {

    private final int maxPerTenant;
    private final TsOverflowPolicy policy;
    private final Map<Long, Integer> counts = new HashMap<>();

    public TsTenantedGuard(int maxPerTenant, TsOverflowPolicy policy) {
        this.maxPerTenant = maxPerTenant;
        this.policy = policy;
    }

    /**
     * Try to admit one series for {@code tenant}. Under
     * {@link TsOverflowPolicy#REJECT} a tenant at its cap throws
     * {@link TsCardinalityException} and is left unchanged; under
     * {@link TsOverflowPolicy#ALLOW} the tenant's count is allowed to climb
     * past the cap.
     */
    public void admit(TsTenantId tenant) {
        int current = counts.getOrDefault(tenant.value(), 0);
        if (current >= maxPerTenant && policy == TsOverflowPolicy.REJECT) {
            throw TsCardinalityException.tenantCap(tenant.value(), maxPerTenant);
        }
        counts.put(tenant.value(), current + 1);
    }

    /** Release one slot for {@code tenant}. Saturates at zero. */
    public void release(TsTenantId tenant) {
        Integer current = counts.get(tenant.value());
        if (current != null && current > 0) {
            counts.put(tenant.value(), current - 1);
        }
    }

    public int count(TsTenantId tenant) {
        return counts.getOrDefault(tenant.value(), 0);
    }

    public int remaining(TsTenantId tenant) {
        return Math.max(0, maxPerTenant - count(tenant));
    }

    public int maxPerTenant() {
        return maxPerTenant;
    }

    /** Tenants seen at least once. Order is arbitrary. */
    public List<TsTenantId> tenants() {
        List<TsTenantId> out = new ArrayList<>();
        for (Long id : counts.keySet()) out.add(new TsTenantId(id));
        return out;
    }

    public int tenantCount() {
        return counts.size();
    }
}
