package com.submillisecond.recipes.ratelimit.features;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;

/**
 * Hierarchical limiter: a parent {@link TokenBucket} caps a shared
 * budget across N child limiters. {@link #tryAcquire(int, long)} must
 * succeed at the child AND the parent.
 *
 * <p>Shape: a multi-tenant API where each tenant has its own per-second
 * rate but a global gateway also has a single ceiling protecting the
 * upstream from a thundering herd of tenants peaking at once.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_rate_limiter::HierarchicalLimiter}.
 */
public final class HierarchicalLimiter {

    private final TokenBucket parent;
    private final List<TokenBucket> children;

    public HierarchicalLimiter(
            long parentCapacity,
            double parentRate,
            int numChildren,
            long childCapacity,
            double childRate) {
        this(parentCapacity, parentRate, numChildren, childCapacity, childRate, SystemClock::new);
    }

    /** Test/advanced constructor. {@code clockSupplier} is called once
     *  per bucket (parent + each child). Tests typically return clones of
     *  a shared {@link TestClock} so every bucket sees the same time. */
    public HierarchicalLimiter(
            long parentCapacity,
            double parentRate,
            int numChildren,
            long childCapacity,
            double childRate,
            Supplier<Clock> clockSupplier) {
        this.parent = new TokenBucket(parentCapacity, parentRate, clockSupplier.get());
        int n = Math.max(1, numChildren);
        this.children = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            children.add(new TokenBucket(childCapacity, childRate, clockSupplier.get()));
        }
    }

    /**
     * Try to acquire {@code n} tokens from {@code childId}. Both child
     * and parent must grant.
     */
    public boolean tryAcquire(int childId, long n) {
        if (childId < 0 || childId >= children.size()) return false;
        TokenBucket child = children.get(childId);
        if (parent.available() < n) return false;
        if (!child.tryAcquire(n)) return false;
        if (parent.tryAcquire(n)) return true;
        // Parent raced us. See Rust sibling: this is the documented
        // best-effort window; child token is leaked here, bounded by
        // parent capacity in long-run terms.
        return false;
    }

    public TokenBucket parent() {
        return parent;
    }

    public TokenBucket child(int childId) {
        if (childId < 0 || childId >= children.size()) return null;
        return children.get(childId);
    }

    public int numChildren() {
        return children.size();
    }
}
