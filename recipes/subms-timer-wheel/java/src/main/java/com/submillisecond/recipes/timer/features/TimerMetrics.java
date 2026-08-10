package com.submillisecond.recipes.timer.features;

import java.util.Objects;

/**
 * Immutable snapshot of per-instance timer wheel counters. Returned by
 * {@link MeteredTimerWheel#metrics()}. {@code cascadeEvents} is 0 for
 * the single-level base wheel; pair with the hierarchical wheel for a
 * non-zero count.
 */
public final class TimerMetrics {

    public final long scheduled;
    public final long fired;
    public final long cancelled;
    public final long rescheduled;
    public final long drained;
    public final long ticks;
    public final long cascadeEvents;

    public TimerMetrics(long scheduled, long fired, long cancelled, long rescheduled,
                        long drained, long ticks, long cascadeEvents) {
        this.scheduled = scheduled;
        this.fired = fired;
        this.cancelled = cancelled;
        this.rescheduled = rescheduled;
        this.drained = drained;
        this.ticks = ticks;
        this.cascadeEvents = cascadeEvents;
    }

    public static TimerMetrics empty() { return new TimerMetrics(0, 0, 0, 0, 0, 0, 0); }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof TimerMetrics that)) return false;
        return scheduled == that.scheduled && fired == that.fired
            && cancelled == that.cancelled && rescheduled == that.rescheduled
            && drained == that.drained && ticks == that.ticks
            && cascadeEvents == that.cascadeEvents;
    }

    @Override
    public int hashCode() {
        return Objects.hash(scheduled, fired, cancelled, rescheduled, drained, ticks, cascadeEvents);
    }

    @Override
    public String toString() {
        return "TimerMetrics{scheduled=" + scheduled + ", fired=" + fired
            + ", cancelled=" + cancelled + ", rescheduled=" + rescheduled
            + ", drained=" + drained + ", ticks=" + ticks
            + ", cascadeEvents=" + cascadeEvents + "}";
    }
}
