package com.submillisecond.recipes.ratelimit.features;

/**
 * Injected monotonic clock. Production wires {@link SystemClock}; tests
 * wire {@link TestClock} to advance time deterministically without
 * wall-clock sleeps.
 *
 * <p>Implementations must be thread-safe and must never go backwards.
 */
public interface Clock {

    /** Nanoseconds since the clock's origin. Monotonic non-decreasing. */
    long nowNs();
}
