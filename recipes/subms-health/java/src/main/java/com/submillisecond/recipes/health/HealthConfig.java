package com.submillisecond.recipes.health;

import com.submillisecond.recipes.events.DispatchMode;

/** Registry-level config: refresh mode + cadence, event dispatch mode, stale factor. */
public final class HealthConfig {
    public enum RefreshMode {
        SYNC,
        ASYNC
    }

    private RefreshMode mode = RefreshMode.ASYNC;
    private long tickMs = 1_000;
    private DispatchMode dispatch = DispatchMode.ASYNC;
    private double staleFactor = 1.5;

    public static HealthConfig defaults() {
        return new HealthConfig();
    }

    /** No background threads: sync refresh + sync dispatch. The low-latency path. */
    public static HealthConfig sync() {
        HealthConfig c = new HealthConfig();
        c.mode = RefreshMode.SYNC;
        c.dispatch = DispatchMode.SYNC;
        return c;
    }

    public HealthConfig mode(RefreshMode mode) {
        this.mode = mode;
        return this;
    }

    public HealthConfig tickMs(long tickMs) {
        this.tickMs = tickMs;
        return this;
    }

    public HealthConfig dispatch(DispatchMode dispatch) {
        this.dispatch = dispatch;
        return this;
    }

    public HealthConfig staleFactor(double factor) {
        this.staleFactor = factor;
        return this;
    }

    public RefreshMode mode() {
        return mode;
    }

    public long tickMs() {
        return tickMs;
    }

    public DispatchMode dispatch() {
        return dispatch;
    }

    public double staleFactor() {
        return staleFactor;
    }
}
