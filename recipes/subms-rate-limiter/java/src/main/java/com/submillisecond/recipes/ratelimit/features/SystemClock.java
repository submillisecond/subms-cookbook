package com.submillisecond.recipes.ratelimit.features;

import com.submillisecond.perf.SubMsTimer;

/**
 * Wall-clock implementation. Origin is the moment the instance is
 * constructed; {@link #nowNs()} returns elapsed ns from the
 * {@link SubMsTimer} against that origin.
 */
public final class SystemClock implements Clock {

    private final long originNs;

    public SystemClock() {
        this.originNs = SubMsTimer.nanosNow();
    }

    @Override
    public long nowNs() {
        return SubMsTimer.nanosNow() - originNs;
    }
}
