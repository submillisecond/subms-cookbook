package com.submillisecond.recipes.timer.features;

import com.submillisecond.recipes.timer.TimerWheel;

import java.util.List;

/**
 * Metered timer wheel: thin wrapper around {@link TimerWheel} that
 * tracks per-instance counters. Counters are plain {@code long} fields
 * - no atomics, no locks - because the underlying wheel is itself
 * single-threaded. Pair with {@link ConcurrentTimerWheel} for a
 * thread-safe metered surface (wrap a {@code MeteredTimerWheel} inside
 * a monitor of your own).
 *
 * <p>{@code cascadeEvents} is always 0 for the single-level base
 * wheel. It's tracked here so downstream code that swaps in
 * {@link HierarchicalTimerWheel} doesn't need a schema change in the
 * metrics snapshot.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_timer_wheel::MeteredTimerWheel}.
 */
public final class MeteredTimerWheel<V> {

    private final TimerWheel<V> wheel;
    private long scheduled;
    private long fired;
    private long cancelled;
    private long ticks;

    public MeteredTimerWheel(int numSlots) {
        this.wheel = new TimerWheel<>(numSlots);
    }

    public int numSlots() { return wheel.numSlots(); }

    public TimerMetrics metrics() {
        return new TimerMetrics(scheduled, fired, cancelled, ticks, 0L);
    }

    public long schedule(int delayTicks, V value) {
        scheduled++;
        return wheel.schedule(delayTicks, value);
    }

    public boolean cancel(long id) {
        boolean ok = wheel.cancel(id);
        if (ok) cancelled++;
        return ok;
    }

    public List<V> tick() {
        ticks++;
        List<V> out = wheel.tick();
        fired += out.size();
        return out;
    }
}
