package com.submillisecond.recipes.hdrhist.features;

/** Deterministic clock for tests. Move time forward with {@link #advanceNs(long)}. */
public final class ManualClock implements Clock {
    private long now;

    public ManualClock() { this.now = 0L; }

    public void advanceNs(long dt) { this.now += dt; }

    @Override public long nowNs() { return now; }
}
