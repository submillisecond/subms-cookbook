package com.submillisecond.recipes.hdrhist.features;

/** Monotonic time source. {@code nowNs()} returns nanoseconds since some arbitrary epoch. */
public interface Clock {
    long nowNs();
}
