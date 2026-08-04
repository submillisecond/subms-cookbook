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

    /**
     * Buffered count from the two published counters. Both loads are acquire
     * because either side may ask, so neither counter is reliably the caller's
     * own.
     */
    private int occupancy() {
        long t = (long) PaddedLong.VH.getAcquire(tail);
        long h = (long) PaddedLong.VH.getAcquire(head);
        return (int) (t - h);
    }

    /** Producer handle. One per buffer; owns the tail. */
    public final class Producer {
        private long cachedHead;

        /**
         * Items currently buffered. A snapshot: the consumer runs concurrently,
         * so the true count can only be lower by the time the caller acts on it.
         * Use it for occupancy alarms and sizing, never to decide whether a push
         * will succeed - {@link #tryPush} already answers that without a race.
         */
        public int size() { return occupancy(); }

        /** True when no items are buffered. Snapshot semantics, as {@link #size}. */
        public boolean isEmpty() { return occupancy() == 0; }

        /** True when the ring holds {@code capacity} items. Snapshot semantics. */
        public boolean isFull() { return occupancy() == capacity; }

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

        /**
         * Bulk-push variant. Copies up to {@code values.length} items into the ring
         * with a single Release on the tail. Returns the count actually pushed.
         * Amortises the per-item atomic cost behind one fence per call.
         *
         * <p>Feature: {@code bulk}.
         */
        public int tryPushBulk(T[] values) {
            if (values == null || values.length == 0) return 0;
            for (T v : values) {
                if (v == null) throw new NullPointerException("null in bulk slice");
            }
            long t = (long) PaddedLong.VH.getOpaque(tail);
            long free = capacity - (t - cachedHead);
            if (free < values.length) {
                cachedHead = (long) PaddedLong.VH.getAcquire(head);
                free = capacity - (t - cachedHead);
            }
            int n = (int) Math.min(free, values.length);
            if (n == 0) return 0;
            for (int i = 0; i < n; i++) {
                buf[(int) ((t + i) & mask)] = values[i];
            }
            PaddedLong.VH.setRelease(tail, t + n);
            return n;
        }
    }

    /** Consumer handle. One per buffer; owns the head. */
    public final class Consumer {
        private long cachedTail;

        /**
         * Items currently buffered. Snapshot semantics, as
         * {@link Producer#size} - the producer runs concurrently, so the true
         * count can only be higher.
         */
        public int size() { return occupancy(); }

        /** True when no items are buffered. Snapshot semantics. */
        public boolean isEmpty() { return occupancy() == 0; }

        /** True when the ring holds {@code capacity} items. Snapshot semantics. */
        public boolean isFull() { return occupancy() == capacity; }

        /** Returns the next value without consuming it, or {@code null} if empty. */
        @SuppressWarnings("unchecked")
        public T peek() {
            long h = (long) PaddedLong.VH.getOpaque(head);
            if (h == cachedTail) {
                cachedTail = (long) PaddedLong.VH.getAcquire(tail);
                if (h == cachedTail) return null;
            }
            return (T) buf[(int) (h & mask)];
        }

        /**
         * Discards every buffered item and returns how many went. Consumer-side
         * only: the producer keeps publishing throughout, so anything it appends
         * after the counters were read stays in the ring.
         */
        public int clear() {
            int n = 0;
            while (tryPop() != null) n++;
            return n;
        }

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

        /**
         * Bulk-pop variant. Drains up to {@code out.length} items into {@code out}
         * with a single Release on the head. Returns the count drained.
         *
         * <p>Feature: {@code bulk}.
         */
        @SuppressWarnings("unchecked")
        public int tryPopBulk(T[] out) {
            if (out == null || out.length == 0) return 0;
            long h = (long) PaddedLong.VH.getOpaque(head);
            long avail = cachedTail - h;
            if (avail < out.length) {
                cachedTail = (long) PaddedLong.VH.getAcquire(tail);
                avail = cachedTail - h;
            }
            int n = (int) Math.min(avail, out.length);
            if (n == 0) return 0;
            for (int i = 0; i < n; i++) {
                int idx = (int) ((h + i) & mask);
                out[i] = (T) buf[idx];
                buf[idx] = null;
            }
            PaddedLong.VH.setRelease(head, h + n);
            return n;
        }
    }

    public Producer producer() { return new Producer(); }
    public Consumer consumer() { return new Consumer(); }
}
