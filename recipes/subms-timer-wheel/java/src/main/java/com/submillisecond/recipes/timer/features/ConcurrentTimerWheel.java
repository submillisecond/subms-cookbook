package com.submillisecond.recipes.timer.features;

import com.submillisecond.recipes.timer.TimerWheel;

import java.util.List;

/**
 * Thread-safe wrapper around {@link TimerWheel}. Schedule, cancel, and
 * tick all serialize on a single monitor because the critical sections
 * are O(1) (or O(bucket) on tick - bounded by entries-in-bucket).
 *
 * <p>Why a monitor and not a lock-free design: timer-wheel operations
 * are short enough that a contended monitor still wins on tail latency
 * vs the cache-line ping-pong of an atomic-list shape, provided callers
 * don't hold long external locks while inside a fired callback. The
 * {@link #tick()} method returns fired values out of the critical
 * section, so dispatch happens after the monitor releases.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_timer_wheel::ConcurrentTimerWheel}.
 */
public final class ConcurrentTimerWheel<V> {

    private final TimerWheel<V> wheel;
    private final Object monitor = new Object();

    public ConcurrentTimerWheel(int numSlots) {
        this.wheel = new TimerWheel<>(numSlots);
    }

    public int numSlots() {
        synchronized (monitor) {
            return wheel.numSlots();
        }
    }

    public long schedule(int delayTicks, V value) {
        synchronized (monitor) {
            return wheel.schedule(delayTicks, value);
        }
    }

    public boolean cancel(long id) {
        synchronized (monitor) {
            return wheel.cancel(id);
        }
    }

    public List<V> tick() {
        synchronized (monitor) {
            return wheel.tick();
        }
    }
}
