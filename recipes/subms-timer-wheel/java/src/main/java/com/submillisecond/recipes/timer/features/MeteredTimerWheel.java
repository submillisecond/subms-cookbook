package com.submillisecond.recipes.timer.features;

import com.submillisecond.recipes.timer.TimerWheel;

import java.util.ArrayList;
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
    private long rescheduled;
    private long drained;
    private long ticks;

    public MeteredTimerWheel(int numSlots) {
        this.wheel = new TimerWheel<>(numSlots);
    }

    public int numSlots() { return wheel.numSlots(); }

    public long maxDelay() { return wheel.maxDelay(); }

    public int pending() { return wheel.pending(); }

    public boolean isEmpty() { return wheel.isEmpty(); }

    public int slotLen(int slot) { return wheel.slotLen(slot); }

    public TimerMetrics metrics() {
        return new TimerMetrics(scheduled, fired, cancelled, rescheduled, drained, ticks, 0L);
    }

    public long schedule(long delayTicks, V value) {
        scheduled++;
        return wheel.schedule(delayTicks, value);
    }

    public long trySchedule(long delayTicks, V value) {
        long id = wheel.trySchedule(delayTicks, value);
        scheduled++;
        return id;
    }

    public boolean cancel(long id) {
        boolean ok = wheel.cancel(id);
        if (ok) cancelled++;
        return ok;
    }

    public boolean reschedule(long id, long delayTicks) {
        boolean ok = wheel.reschedule(id, delayTicks);
        if (ok) rescheduled++;
        return ok;
    }

    public List<V> tick() {
        ticks++;
        List<V> out = wheel.tick();
        fired += out.size();
        return out;
    }

    public List<V> advance(int n) {
        List<V> out = new ArrayList<>();
        for (int i = 0; i < n; i++) out.addAll(tick());
        return out;
    }

    /**
     * Hand back every pending timer. Counted apart from {@code fired}: a
     * drained timer never came due, and folding the two together would make a
     * shutdown look like a burst of expiries.
     */
    public List<V> drain() {
        List<V> out = wheel.drain();
        drained += out.size();
        return out;
    }

    public void clear() {
        drained += wheel.pending();
        wheel.clear();
    }
}
