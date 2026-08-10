package com.submillisecond.recipes.timer.features;

import com.submillisecond.recipes.timer.TimerWheel;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;

/**
 * Absolute-deadline scheduling layer on top of {@link TimerWheel}.
 * Callers schedule against monotonic instants ("fire at t=...") and
 * drive the scheduler with {@link #poll()} calls. The wheel itself
 * stays tick-counted; the layer translates between instant deltas and
 * tick deltas via an injected {@link Clock} so the workload is
 * deterministic under test.
 *
 * <p>Granularity is 1 ms per tick by default; a deadline of {@code now
 * + 12 ms} lands twelve ticks out. Sub-ms deadlines round up to one
 * tick.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_timer_wheel::DeadlineScheduler}.
 */
public final class DeadlineScheduler<V> {

    private final TimerWheel<V> wheel;
    private final Clock clock;
    private final long tickNanos;
    private long consumedNanos;

    public DeadlineScheduler(int numSlots, Clock clock, Duration tick) {
        this.wheel = new TimerWheel<>(numSlots);
        this.clock = clock;
        this.tickNanos = Math.max(1, tick.toNanos());
    }

    public long tickNanos() { return tickNanos; }
    public Clock clock() { return clock; }

    public int pending() { return wheel.pending(); }

    public boolean isEmpty() { return wheel.isEmpty(); }

    /** Schedule {@code value} to fire after {@code delay}. */
    public long scheduleAfter(Duration delay, V value) {
        int ticks = nanosToTicks(delay.toNanos());
        return wheel.schedule(ticks, value);
    }

    /**
     * Schedule {@code value} to fire at absolute deadline
     * {@code whenNanos} (same epoch as {@link Clock#nowNanos()}). If
     * the deadline is in the past, the timer is queued for the next
     * tick.
     */
    public long scheduleAt(long whenNanos, V value) {
        long now = clock.nowNanos();
        long diff = Math.max(0, whenNanos - now);
        int ticks = Math.max(1, nanosToTicks(diff));
        return wheel.schedule(ticks, value);
    }

    public boolean cancel(long id) {
        return wheel.cancel(id);
    }

    /**
     * Push a pending timer out to a new deadline, keeping its id. This is the
     * idle-timeout pattern: one timer per session, bumped on every inbound
     * message rather than cancelled and re-armed.
     */
    public boolean rescheduleAt(long id, long whenNanos) {
        long now = clock.nowNanos();
        long diff = Math.max(0, whenNanos - now);
        int ticks = Math.max(1, nanosToTicks(diff));
        return wheel.reschedule(id, ticks);
    }

    public boolean rescheduleAfter(long id, Duration delay) {
        int ticks = Math.max(1, nanosToTicks(delay.toNanos()));
        return wheel.reschedule(id, ticks);
    }

    /** Hand back every armed timer without firing it. The shutdown path. */
    public List<V> drain() {
        return wheel.drain();
    }

    /**
     * Advance the wheel by however many ticks the clock has accrued
     * since the last {@code poll()}. Returns every fired value across
     * the catch-up batch. Idempotent if called twice with no clock
     * movement in between.
     */
    public List<V> poll() {
        long now = clock.nowNanos();
        long pending = Math.max(0, now - consumedNanos);
        int ticks = (int) Math.min(Integer.MAX_VALUE, pending / tickNanos);
        consumedNanos += (long) ticks * tickNanos;
        List<V> fired = new ArrayList<>();
        for (int i = 0; i < ticks; i++) {
            fired.addAll(wheel.tick());
        }
        return fired;
    }

    private int nanosToTicks(long nanos) {
        long t = (nanos + tickNanos - 1) / tickNanos;
        if (t > Integer.MAX_VALUE) return Integer.MAX_VALUE;
        return (int) t;
    }
}
