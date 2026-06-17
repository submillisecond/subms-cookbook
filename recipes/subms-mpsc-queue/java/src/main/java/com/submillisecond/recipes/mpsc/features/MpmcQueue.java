package com.submillisecond.recipes.mpsc.features;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Multi-consumer extension: bounded MPMC ring with tail-sequence CAS.
 *
 * <p>Disruptor-style barrier: per-slot sequence numbers gate both
 * producer claim (CAS the tail) and consumer claim (CAS the head).
 * Multiple consumers race; the loser sees a stale head and retries.
 *
 * <p>Both {@link #tryEnqueue(Object)} and {@link #tryDequeue()} are
 * wait-free in the uncontended case and bounded-retry under
 * contention.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_mpsc_queue::MpmcQueue}.
 *
 * @param <T> element type; nulls are not permitted.
 */
public final class MpmcQueue<T> {

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
    private final AtomicLong head = new AtomicLong(0);
    private final AtomicLong tail = new AtomicLong(0);
    private final AtomicLong casRetries = new AtomicLong(0);

    public MpmcQueue(int capacity) {
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

    /** Total CAS-loss count across producers + consumers. */
    public long casRetries() {
        return casRetries.get();
    }

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
                casRetries.incrementAndGet();
                t = tail.get();
            } else if (diff < 0) {
                return false;
            } else {
                t = tail.get();
            }
        }
    }

    @SuppressWarnings("unchecked")
    public T tryDequeue() {
        long h = head.get();
        while (true) {
            Slot slot = slots[(int) (h & mask)];
            long seq = (long) Slot.SEQ.getAcquire(slot);
            long diff = seq - (h + 1);
            if (diff == 0) {
                if (head.compareAndSet(h, h + 1)) {
                    T v = (T) slot.value;
                    slot.value = null;
                    Slot.SEQ.setRelease(slot, h + (mask + 1L));
                    return v;
                }
                casRetries.incrementAndGet();
                h = head.get();
            } else if (diff < 0) {
                return null;
            } else {
                h = head.get();
            }
        }
    }

    public int size() {
        long h = head.getAcquire();
        long t = tail.getAcquire();
        return (int) (t - h);
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
