package com.submillisecond.recipes.ratelimit.features;

/**
 * Cross-process state backend. Implementations bump a counter for
 * {@code (key, windowStart)} and return the new value after the bump.
 *
 * <p>The trait abstracts the atomic INCR + EXPIRE primitive shared by
 * Redis / Memcached / DynamoDB. Real distributed implementations are
 * downstream user concerns; this recipe ships an
 * {@link InMemoryBackend}.
 */
public interface Backend {

    /**
     * Increment the counter at {@code key} for {@code windowStartNs}.
     * Returns the new count after the bump. Must be atomic across
     * concurrent callers.
     */
    long incr(String key, long windowStartNs, long ttlNs);

    /**
     * Read the current counter without bumping. Returns 0 if the
     * (key, window) pair is unknown or expired.
     */
    long read(String key, long windowStartNs);
}
