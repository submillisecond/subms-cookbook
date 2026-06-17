package com.submillisecond.recipes.ratelimit.features;

import java.util.concurrent.atomic.AtomicLong;

/**
 * Deterministic clock for tests. {@link #advance(long)} moves the clock
 * forward; {@link #nowNs()} reads the current value.
 */
public final class TestClock implements Clock {

    private final AtomicLong now;

    public TestClock() {
        this(0L);
    }

    public TestClock(long startNs) {
        this.now = new AtomicLong(startNs);
    }

    public void advance(long ns) {
        if (ns < 0) throw new IllegalArgumentException("ns must be >= 0");
        now.addAndGet(ns);
    }

    public void advanceMs(long ms) {
        advance(Math.multiplyExact(ms, 1_000_000L));
    }

    @Override
    public long nowNs() {
        return now.get();
    }
}
