package com.submillisecond.recipes.spsc;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;

/**
 * Wait-free single-producer single-consumer ring buffer.
 *
 * <p>Power-of-two capacity (rounded up from the request). Index modulo via
 * bitmask, not {@code %}. Head and tail counters live in padded inner objects
 * so the producer's writes don't invalidate the consumer's read line.
 *
 * <p>Each side caches the opposite index and only re-reads through the
 * {@link VarHandle} when its cache says full / empty.
 *
 * <p>Memory ordering: release-store on publish (tail/head writer side),
 * acquire-load on the opposite-side read, opaque/relaxed on own-side reads.
 *
 * <p>Use {@link #producer()} and {@link #consumer()} once, move each handle
 * to its owning thread; the type system does not enforce SPSC the way Rust
 * does, so by convention treat them as single-thread-owned.
 *
 * @param <T> element type; nulls are not permitted.
 */
public final class SpscRingBuffer<T> {

    /** Padded counter cell. 128 bytes of front padding keeps the AtomicLong line solo. */
    private static final class PaddedLong {
        @SuppressWarnings("unused")
        long p1, p2, p3, p4, p5, p6, p7;
        volatile long value;
        @SuppressWarnings("unused")
        long q1, q2, q3, q4, q5, q6, q7;

        static final VarHandle VH;
        static {
            try {
                VH = MethodHandles.lookup().findVarHandle(PaddedLong.class, "value", long.class);
            } catch (ReflectiveOperationException e) {
                throw new ExceptionInInitializerError(e);
            }
        }
    }

    private final PaddedLong head = new PaddedLong();
    private final PaddedLong tail = new PaddedLong();
    private final int capacity;
    private final int mask;
    private final Object[] buf;

    public SpscRingBuffer(int requestedCapacity) {
        int cap = Math.max(2, requestedCapacity);
        cap = Integer.highestOneBit(cap - 1) << 1;
        this.capacity = cap;
        this.mask = cap - 1;
        this.buf = new Object[cap];
    }

    public int capacity() {
        return capacity;
    }

    /** Producer handle. One per buffer; owns the tail. */
    public final class Producer {
        private long cachedHead;

        /** Returns {@code true} on success, {@code false} if the buffer is full. */
        public boolean tryPush(T value) {
            if (value == null) throw new NullPointerException();
            long t = (long) PaddedLong.VH.getOpaque(tail);
            if (t - cachedHead == capacity) {
                cachedHead = (long) PaddedLong.VH.getAcquire(head);
                if (t - cachedHead == capacity) return false;
            }
            buf[(int) (t & mask)] = value;
            PaddedLong.VH.setRelease(tail, t + 1);
            return true;
        }
    }

    /** Consumer handle. One per buffer; owns the head. */
    public final class Consumer {
        private long cachedTail;

        /** Returns the next value or {@code null} if the buffer is empty. */
        @SuppressWarnings("unchecked")
        public T tryPop() {
            long h = (long) PaddedLong.VH.getOpaque(head);
            if (h == cachedTail) {
                cachedTail = (long) PaddedLong.VH.getAcquire(tail);
                if (h == cachedTail) return null;
            }
            int idx = (int) (h & mask);
            Object v = buf[idx];
            buf[idx] = null; // release reference for GC
            PaddedLong.VH.setRelease(head, h + 1);
            return (T) v;
        }
    }

    public Producer producer() { return new Producer(); }
    public Consumer consumer() { return new Consumer(); }
}
