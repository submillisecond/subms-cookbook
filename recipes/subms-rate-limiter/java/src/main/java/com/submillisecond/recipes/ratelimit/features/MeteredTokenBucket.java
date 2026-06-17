package com.submillisecond.recipes.ratelimit.features;

import java.util.concurrent.atomic.AtomicLong;

/**
 * Metered token bucket: wraps {@link TokenBucket} and tracks
 * per-instance counters - acquires granted, acquires rejected, refill
 * events, current token level - so consumers can scrape the limiter as
 * a metric source without a separate observability layer.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_rate_limiter::MeteredTokenBucket}.
 */
public final class MeteredTokenBucket {

    private final TokenBucket inner;
    private final AtomicLong granted = new AtomicLong(0L);
    private final AtomicLong rejected = new AtomicLong(0L);
    private final AtomicLong refills = new AtomicLong(0L);
    private final AtomicLong lastAvailable;

    public MeteredTokenBucket(long capacity, double ratePerSec) {
        this(capacity, ratePerSec, new SystemClock());
    }

    public MeteredTokenBucket(long capacity, double ratePerSec, Clock clock) {
        this.inner = new TokenBucket(capacity, ratePerSec, clock);
        this.lastAvailable = new AtomicLong(inner.available());
    }

    public boolean tryAcquire(long n) {
        // `available()` itself triggers refill, so it gives the
        // post-refill snapshot. Compare against the last seen value to
        // detect a refill step.
        long last = lastAvailable.get();
        long postRefillBefore = inner.available();
        if (postRefillBefore > last) {
            refills.incrementAndGet();
        }
        boolean ok = inner.tryAcquire(n);
        long after = inner.available();
        if (ok) {
            granted.incrementAndGet();
        } else {
            rejected.incrementAndGet();
        }
        lastAvailable.set(after);
        return ok;
    }

    public boolean tryAcquireOne() {
        return tryAcquire(1L);
    }

    public MetricsSnapshot snapshot() {
        return new MetricsSnapshot(
                granted.get(),
                rejected.get(),
                refills.get(),
                inner.available());
    }

    public long capacity() {
        return inner.capacity();
    }

    public double ratePerSec() {
        return inner.ratePerSec();
    }
}
