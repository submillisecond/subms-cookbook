package com.submillisecond.recipes.timer.features;

import java.util.ArrayList;
import java.util.List;

/**
 * Hierarchical timer wheel (HHW). Three levels, each a wheel of 64 slots:
 * seconds, minutes (each slot = 64 ticks), hours (each slot = 64*64
 * ticks). A timer scheduled {@code d} ticks out lands on the coarsest
 * wheel whose slot can hold it; on each tick of a higher wheel we
 * cascade its expiring slot's entries down to the lower wheel re-binned
 * at the residual offset.
 *
 * <p>Capacity: 64 * 64 * 64 = 262_144 ticks. Long delays no longer pay
 * a no-op revolution per {@code mask} ticks the way the base wheel
 * does; they sit on the coarse wheel and get cascaded down only as
 * their fire time approaches.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_timer_wheel::HierarchicalTimerWheel}.
 */
public final class HierarchicalTimerWheel<V> {

    private static final int LEVELS = 3;
    private static final int SLOTS = 64;
    private static final int MASK = SLOTS - 1;
    private static final int[] LEVEL_SHIFT = {0, 6, 12};
    private static final long[] LEVEL_RANGE = {
        SLOTS,
        (long) SLOTS * SLOTS,
        (long) SLOTS * SLOTS * SLOTS,
    };

    private static final class Entry<V> {
        final long id;
        final long deadline;
        V value;
        boolean cancelled;
        Entry(long id, long deadline, V value) { this.id = id; this.deadline = deadline; this.value = value; }
    }

    /** wheels[level][slot] = list of entries. */
    private final List<List<Entry<V>>>[] wheels;
    private long now;
    private long nextId = 1;
    private long cascades;

    @SuppressWarnings("unchecked")
    public HierarchicalTimerWheel() {
        this.wheels = (List<List<Entry<V>>>[]) new List<?>[LEVELS];
        for (int l = 0; l < LEVELS; l++) {
            List<List<Entry<V>>> level = new ArrayList<>(SLOTS);
            for (int s = 0; s < SLOTS; s++) level.add(new ArrayList<>());
            wheels[l] = level;
        }
    }

    public long now() { return now; }
    public long cascades() { return cascades; }

    public static long maxDelay() { return LEVEL_RANGE[LEVELS - 1]; }

    /**
     * Schedule {@code value} to fire in {@code delay} ticks. Delays larger
     * than {@link #maxDelay()} are clamped to the cap.
     */
    public long schedule(long delay, V value) {
        long cap = maxDelay();
        long d = Math.min(delay, cap - 1);
        return trySchedule(d, value);
    }

    /**
     * Returns -1 if the delay overflows the wheel's capacity; otherwise
     * returns the scheduled id.
     */
    public long trySchedule(long delay, V value) {
        if (delay >= maxDelay()) return -1L;
        long deadline = now + delay;
        long id = nextId++;
        Entry<V> e = new Entry<>(id, deadline, value);
        int[] lvlSlot = bucketFor(deadline);
        wheels[lvlSlot[0]].get(lvlSlot[1]).add(e);
        return id;
    }

    /**
     * Mark {@code id} cancelled. Linear sweep over buckets - faster
     * than maintaining an id-to-slot map that would need patching on
     * every cascade.
     */
    public boolean cancel(long id) {
        for (int l = 0; l < LEVELS; l++) {
            for (int s = 0; s < SLOTS; s++) {
                for (Entry<V> e : wheels[l].get(s)) {
                    if (e.id == id && !e.cancelled) {
                        e.cancelled = true;
                        e.value = null;
                        return true;
                    }
                }
            }
        }
        return false;
    }

    /**
     * Advance one tick. Returns the values of all timers whose
     * deadline equals the new {@code now}.
     */
    public List<V> tick() {
        now++;
        // Cascade higher levels whose slot is about to roll over.
        // Walk highest-to-lowest so a level-2 entry cascading down to
        // level 1 still has time to re-cascade to level 0 on the same
        // tick when its deadline is now.
        for (int lvl = LEVELS - 1; lvl >= 1; lvl--) {
            long lowerPeriod = 1L << LEVEL_SHIFT[lvl];
            if (now % lowerPeriod == 0) {
                int slot = (int) ((now >>> LEVEL_SHIFT[lvl]) & MASK);
                List<Entry<V>> entries = wheels[lvl].get(slot);
                wheels[lvl].set(slot, new ArrayList<>());
                for (Entry<V> e : entries) {
                    if (e.cancelled) continue;
                    cascades++;
                    int[] lvlSlot = bucketFor(e.deadline);
                    wheels[lvlSlot[0]].get(lvlSlot[1]).add(e);
                }
            }
        }
        int slot = (int) (now & MASK);
        List<Entry<V>> entries = wheels[0].get(slot);
        wheels[0].set(slot, new ArrayList<>());
        List<V> fired = new ArrayList<>();
        for (Entry<V> e : entries) {
            if (e.cancelled || e.deadline != now) continue;
            V v = e.value;
            e.value = null;
            if (v != null) fired.add(v);
        }
        return fired;
    }

    private int[] bucketFor(long deadline) {
        long diff = Math.max(0, deadline - now);
        int lvl;
        if (diff < LEVEL_RANGE[0]) lvl = 0;
        else if (diff < LEVEL_RANGE[1]) lvl = 1;
        else lvl = 2;
        int slot = (int) ((deadline >>> LEVEL_SHIFT[lvl]) & MASK);
        return new int[] {lvl, slot};
    }
}
