package com.submillisecond.recipes.timer.features;

/**
 * Wall-clock {@link Clock} backed by {@link System#nanoTime()}.
 * The origin is fixed at construction so {@link #nowNanos()} starts
 * near zero.
 */
public final class MonotonicClock implements Clock {

    private final long origin = System.nanoTime();

    @Override
    public long nowNanos() {
        return System.nanoTime() - origin;
    }
}
