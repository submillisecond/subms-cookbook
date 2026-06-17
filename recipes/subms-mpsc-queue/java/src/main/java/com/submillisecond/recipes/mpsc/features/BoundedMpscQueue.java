package com.submillisecond.recipes.mpsc.features;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Bounded MPSC queue: fixed-capacity ring buffer with backpressure.
 *
 * <p>Producers see backpressure via {@link #tryEnqueue(Object)} returning
 * {@code false} when the ring is full. Single consumer only.
 *
 * <p>Layout: power-of-two capacity, per-slot sequence numbers. Producers
 * CAS the tail to claim a slot, then publish via a release-store of the
 * slot's sequence. The consumer reads slots in order and advances head
 * once each is consumed.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_mpsc_queue::BoundedMpscQueue}.
 *
 * @param <T> element type; nulls are not permitted.
 */
public final class BoundedMpscQueue<T> {

    private static final class Slot {
        volatile long seq;
        Object value;
        static final VarHandle SEQ;
        static {
            try {
                SEQ = MethodHandles.lookup().findVarHandle(Slot.class, "seq", long.class);
            } catch (ReflectiveOperationException e) {
                throw new ExceptionInInitializerError(e);
            }
        }
    }

    private final int mask;
    private final Slot[] slots;
    private final AtomicLong tail = new AtomicLong(0);
    /** Consumer-private; updated only by the single consumer. */
    private long head = 0;

    public BoundedMpscQueue(int capacity) {
        int cap = nextPow2(Math.max(2, capacity));
        this.mask = cap - 1;
        this.slots = new Slot[cap];
        for (int i = 0; i < cap; i++) {
            Slot s = new Slot();
            s.seq = i;
            slots[i] = s;
        }
    }

    public int capacity() {
        return mask + 1;
    }

    /**
     * Multi-producer push. Returns {@code true} on success or
     * {@code false} when the ring is full (the value is not retained).
     */
    public boolean tryEnqueue(T value) {
        if (value == null) throw new NullPointerException();
        long t = tail.get();
        while (true) {
            Slot slot = slots[(int) (t & mask)];
            long seq = (long) Slot.SEQ.getAcquire(slot);
            long diff = seq - t;
            if (diff == 0) {
                if (tail.compareAndSet(t, t + 1)) {
                    slot.value = value;
                    Slot.SEQ.setRelease(slot, t + 1);
                    return true;
                }
                t = tail.get();
            } else if (diff < 0) {
                return false;
            } else {
                t = tail.get();
            }
        }
    }

    /** Single-consumer pop. Returns {@code null} when the ring is empty. */
    @SuppressWarnings("unchecked")
    public T tryDequeue() {
        Slot slot = slots[(int) (head & mask)];
        long seq = (long) Slot.SEQ.getAcquire(slot);
        long diff = seq - (head + 1);
        if (diff == 0) {
            T v = (T) slot.value;
            slot.value = null;
            // Mark slot open for the next producer pass.
            Slot.SEQ.setRelease(slot, head + (mask + 1L));
            head++;
            return v;
        }
        return null;
    }

    /** Best-effort outstanding-item count. */
    public int size() {
        long t = (long) tail.getAcquire();
        return (int) (t - head);
    }

    public boolean isEmpty() {
        return size() == 0;
    }

    private static int nextPow2(int n) {
        int x = 1;
        while (x < n) x <<= 1;
        return x;
    }
}
