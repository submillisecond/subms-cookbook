package com.submillisecond.recipes.timer.features;

/**
 * Source of monotonic time for {@link DeadlineScheduler}. Production
 * uses {@link MonotonicClock}; tests inject {@link TestClock}.
 */
public interface Clock {
    /** Elapsed monotonic nanoseconds since the clock's origin. */
    long nowNanos();
}
