package com.submillisecond.recipes.timer.features;

import java.time.Duration;

/**
 * Hand-stepped {@link Clock} for deterministic tests. Move time
 * forward with {@link #advance(Duration)}; the scheduler catches up
 * via {@link DeadlineScheduler#poll()}.
 */
public final class TestClock implements Clock {

    private long nowNanos;

    @Override
    public long nowNanos() {
        return nowNanos;
    }

    public void advance(Duration d) {
        long add = d.toNanos();
        long next = nowNanos + add;
        if (next < nowNanos) next = Long.MAX_VALUE;
        nowNanos = next;
    }
}
