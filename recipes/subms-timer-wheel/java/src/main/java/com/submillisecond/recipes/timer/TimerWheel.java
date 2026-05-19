package com.submillisecond.recipes.timer;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Single-level hashed timer wheel. O(1) schedule and cancel.
 *
 * <p>A scheduled timer at delay <code>d</code> ticks goes into bucket
 * <code>(hand + d) % N</code> with a rounds counter of <code>d / N</code>.
 * On {@link #tick()}, the hand advances one bucket; timers with rounds==0
 * fire; the rest have rounds decremented. Cancel sets a flag; the entry is
 * dropped lazily on the next visit to that bucket.
 */
public final class TimerWheel<V> {

    private static final class Entry<V> {
        final long id;
        int rounds;
        V value;
        boolean cancelled;
        Entry(long id, int rounds, V value) { this.id = id; this.rounds = rounds; this.value = value; }
    }

    private final List<List<Entry<V>>> slots;
    private final int mask;
    private int hand;
    private long nextId = 1;
    private final Map<Long, Integer> idToSlot = new HashMap<>();

    public TimerWheel(int numSlots) {
        int n = Math.max(2, numSlots);
        n = Integer.highestOneBit(n - 1) << 1;
        this.slots = new ArrayList<>(n);
        for (int i = 0; i < n; i++) slots.add(new ArrayList<>());
        this.mask = n - 1;
    }

    public int numSlots() { return slots.size(); }

    /** Schedule {@code value} to fire in {@code delayTicks}. Returns the id. */
    public long schedule(int delayTicks, V value) {
        int target = (hand + delayTicks) & mask;
        int rounds = delayTicks / slots.size();
        long id = nextId++;
        slots.get(target).add(new Entry<>(id, rounds, value));
        idToSlot.put(id, target);
        return id;
    }

    /** Mark a timer cancelled. Returns true if it was pending. */
    public boolean cancel(long id) {
        Integer slot = idToSlot.get(id);
        if (slot == null) return false;
        for (Entry<V> e : slots.get(slot)) {
            if (e.id == id && !e.cancelled) {
                e.cancelled = true;
                return true;
            }
        }
        return false;
    }

    /** Advance one tick. Returns the values of all timers that fired. */
    public List<V> tick() {
        hand = (hand + 1) & mask;
        List<Entry<V>> entries = slots.get(hand);
        slots.set(hand, new ArrayList<>());
        List<V> fired = new ArrayList<>();
        List<Entry<V>> survivors = new ArrayList<>();
        for (Entry<V> e : entries) {
            if (e.cancelled) {
                idToSlot.remove(e.id);
                continue;
            }
            if (e.rounds == 0) {
                idToSlot.remove(e.id);
                fired.add(e.value);
            } else {
                e.rounds--;
                survivors.add(e);
            }
        }
        slots.set(hand, survivors);
        return fired;
    }
}
