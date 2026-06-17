package com.submillisecond.recipes.ratelimit.features;

/**
 * Rate limiter backed by a pluggable {@link Backend}. Fixed-window
 * algorithm: per {@code (key, windowSizeNs)}, allow at most
 * {@code limit} requests.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_rate_limiter::DistributedLimiter}.
 */
public final class DistributedLimiter {

    private final Backend backend;
    private final Clock clock;
    private final long limit;
    private final long windowNs;

    public DistributedLimiter(Backend backend, long limit, long windowNs) {
        this(backend, limit, windowNs, new SystemClock());
    }

    public DistributedLimiter(Backend backend, long limit, long windowNs, Clock clock) {
        this.backend = backend;
        this.clock = clock;
        this.limit = Math.max(1L, limit);
        this.windowNs = Math.max(1L, windowNs);
    }

    /**
     * Try to acquire one permit on {@code key}. Returns true iff the
     * post-bump counter is within {@code limit}. The bump always
     * happens (mirroring Redis INCR + EXPIRE) so contention races
     * resolve monotonically.
     */
    public boolean tryAcquire(String key) {
        long now = clock.nowNs();
        long windowStart = now - (now % windowNs);
        long count = backend.incr(key, windowStart, windowNs);
        return count <= limit;
    }

    public long limit() {
        return limit;
    }

    public long windowNs() {
        return windowNs;
    }
}
