package com.submillisecond.recipes.timer;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Single-level hashed timer wheel. O(1) schedule and cancel.
 *
 * <p>A scheduled timer at delay <code>d</code> ticks goes into bucket
 * <code>(hand + d) % N</code> with a rounds counter of how many full
 * revolutions it must sit out first. On {@link #tick()}, the hand advances one
 * bucket; timers with rounds==0 fire; the rest have rounds decremented. Cancel
 * drops the id from the index and flags the entry; the flagged entry is
 * reclaimed on the next visit to that bucket.
 *
 * <p>Thread safety: this class is not synchronised. One caller owns the wheel
 * and there is no interior locking to pay for. To arm timers from several
 * threads at once, use {@code ConcurrentTimerWheel} or hand work to the ticker
 * thread through a queue.
 *
 * <p>Byte-equivalent to the Rust sibling {@code subms_timer_wheel::TimerWheel}.
 */
public final class TimerWheel<V> {

    /**
     * Ceiling on a timer's rounds counter. Matches the Rust port, whose
     * counter is a {@code u32} held to the same bound, so both ports refuse
     * exactly the same delays.
     */
    private static final long MAX_ROUNDS = Integer.MAX_VALUE;

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
    /**
     * Live timers only: an id leaves this index the moment it is cancelled or
     * fired, so {@link #pending()} never counts a retired timer.
     */
    private final Map<Long, Integer> idToSlot = new HashMap<>();

    public TimerWheel(int numSlots) {
        int n = Math.max(2, numSlots);
        n = Integer.highestOneBit(n - 1) << 1;
        this.slots = new ArrayList<>(n);
        for (int i = 0; i < n; i++) slots.add(new ArrayList<>());
        this.mask = n - 1;
    }

    public int numSlots() { return slots.size(); }

    /**
     * Largest delay the wheel can represent: a timer can sit out at most
     * {@code MAX_ROUNDS} revolutions of N slots.
     */
    public long maxDelay() { return (long) slots.size() * MAX_ROUNDS; }

    /**
     * Live (scheduled, not yet fired or cancelled) timers. A correct wheel
     * returns this to 0 once every scheduled timer has fired; a leak would let
     * it climb without bound.
     */
    public int pending() { return idToSlot.size(); }

    public boolean isEmpty() { return idToSlot.isEmpty(); }

    /**
     * Entries physically held in one bucket, including cancelled ones not yet
     * swept. Reading the spread across buckets is how you catch a workload
     * whose delays all collide on one slot.
     */
    public int slotLen(int slot) {
        if (slot < 0 || slot >= slots.size()) return 0;
        return slots.get(slot).size();
    }

    /**
     * Schedule {@code value} to fire in {@code delayTicks}. Returns the id.
     *
     * <p>A delay of 0 or less fires on the next tick, matching Netty's
     * treatment of a deadline already in the past. A delay past
     * {@link #maxDelay()} is clamped; use {@link #trySchedule} to have it
     * refused instead.
     */
    public long schedule(long delayTicks, V value) {
        long id = nextId++;
        insert(id, clampDelay(delayTicks), value);
        return id;
    }

    /** Schedule {@code value}, refusing a delay the wheel cannot represent. */
    public long trySchedule(long delayTicks, V value) {
        long max = maxDelay();
        if (delayTicks > max) throw TimerError.delayTooLong(delayTicks, max);
        return schedule(delayTicks, value);
    }

    /** Mark a timer cancelled. Returns true if it was pending. */
    public boolean cancel(long id) {
        Integer slot = idToSlot.remove(id);
        if (slot == null) return false;
        // The entry keeps its seat until the hand reaches this bucket; only the
        // index is updated eagerly, which is what keeps pending() exact.
        for (Entry<V> e : slots.get(slot)) {
            if (e.id == id && !e.cancelled) {
                e.cancelled = true;
                e.value = null;
                return true;
            }
        }
        return false;
    }

    /**
     * Move a pending timer to a new delay, keeping its id. Returns false if
     * the id is not pending (already fired, already cancelled, unknown).
     *
     * <p>Unlike cancel this removes the entry eagerly - leaving a flagged
     * entry behind would let one id sit in two buckets at once.
     */
    public boolean reschedule(long id, long delayTicks) {
        Integer slot = idToSlot.remove(id);
        if (slot == null) return false;
        List<Entry<V>> bucket = slots.get(slot);
        int pos = -1;
        for (int i = 0; i < bucket.size(); i++) {
            Entry<V> e = bucket.get(i);
            if (e.id == id && !e.cancelled) { pos = i; break; }
        }
        if (pos < 0) return false;
        Entry<V> entry = bucket.get(pos);
        int last = bucket.size() - 1;
        if (pos != last) bucket.set(pos, bucket.get(last));
        bucket.remove(last);
        insert(id, clampDelay(delayTicks), entry.value);
        return true;
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

    /**
     * Advance {@code ticks} ticks and return everything that fired across
     * them, in tick order. A ticker thread that woke late catches up here
     * rather than firing a whole revolution's timers on one bucket.
     */
    public List<V> advance(int ticks) {
        List<V> fired = new ArrayList<>();
        for (int i = 0; i < ticks; i++) fired.addAll(tick());
        return fired;
    }

    /**
     * Remove every pending timer and return its value. The hand stays where it
     * is. This is the shutdown path: Netty's {@code HashedWheelTimer.stop}
     * hands back the timeouts it never got to run, and so does this.
     */
    public List<V> drain() {
        List<V> out = new ArrayList<>(idToSlot.size());
        for (int i = 0; i < slots.size(); i++) {
            for (Entry<V> e : slots.get(i)) {
                if (!e.cancelled) out.add(e.value);
            }
            slots.set(i, new ArrayList<>());
        }
        idToSlot.clear();
        return out;
    }

    /**
     * Drop every pending timer and reset the hand. Ids already handed out are
     * never reused, so a late cancel on a cleared timer returns false rather
     * than hitting an unrelated timer.
     */
    public void clear() {
        for (int i = 0; i < slots.size(); i++) slots.get(i).clear();
        idToSlot.clear();
        hand = 0;
    }

    private long clampDelay(long delayTicks) {
        return Math.max(1L, Math.min(delayTicks, maxDelay()));
    }

    private void insert(long id, long delay, V value) {
        long n = slots.size();
        int slot = (int) ((hand + delay) & mask);
        // rounds = ceil(d/N) - 1, not floor(d/N). They agree everywhere except
        // when d is an exact multiple of N, where the timer lands back on the
        // bucket the hand has just left and waits a full revolution for the
        // revisit. Charging a rounds counter for that revolution as well fires
        // the timer a lap late.
        int rounds = (int) ((delay + n - 1) / n - 1);
        slots.get(slot).add(new Entry<>(id, rounds, value));
        idToSlot.put(id, slot);
    }
}
