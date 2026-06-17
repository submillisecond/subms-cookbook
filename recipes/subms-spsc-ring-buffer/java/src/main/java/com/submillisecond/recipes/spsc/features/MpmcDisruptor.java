package com.submillisecond.recipes.spsc.features;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Multi-producer multi-consumer ring with sequence-barrier gating
 * (LMAX-Disruptor-style).
 *
 * <p>Three sequences:
 *
 * <ul>
 *   <li>{@code producerCursor}: next index a producer will try to claim.
 *       CAS'd by {@link Producer#tryPublish}.
 *   <li>{@code published[idx]}: per-slot atomic published flag holding the
 *       absolute sequence value of the last publish (distinguishes round N
 *       from round N+1 in the same slot).
 *   <li>{@code consumerCursors[i]}: per-consumer next index to read.
 *       Producers gate behind the slowest consumer.
 * </ul>
 *
 * <p>Independent of the base SPSC structures - this is a separate ring with
 * different invariants. All consumers see every published item (broadcast).
 */
public final class MpmcDisruptor<T> {

    private static final long UNPUBLISHED = -1L;

    private final Object[] buf;
    private final AtomicLong[] published;
    private final AtomicLong producerCursor = new AtomicLong(-1L);
    private final AtomicLong[] consumerCursors;
    private final int capacity;
    private final long mask;

    /** Build with {@code consumerCount} consumers and at least
     * {@code requestedCapacity} slots (rounded up to power of two, floor 2). */
    public MpmcDisruptor(int requestedCapacity, int consumerCount) {
        if (consumerCount < 1) throw new IllegalArgumentException("need at least one consumer");
        int cap = Math.max(2, requestedCapacity);
        cap = Integer.highestOneBit(cap - 1) << 1;
        this.capacity = cap;
        this.mask = cap - 1;
        this.buf = new Object[cap];
        this.published = new AtomicLong[cap];
        for (int i = 0; i < cap; i++) {
            this.published[i] = new AtomicLong(UNPUBLISHED);
        }
        this.consumerCursors = new AtomicLong[consumerCount];
        for (int i = 0; i < consumerCount; i++) {
            this.consumerCursors[i] = new AtomicLong(-1L);
        }
    }

    public int capacity() { return capacity; }
    public int consumerCount() { return consumerCursors.length; }

    /** Build a producer handle. Cheap; share across producer threads. */
    public Producer producer() { return new Producer(); }

    /** Build a consumer handle for index {@code i} (0..consumerCount-1). */
    public Consumer consumer(int i) {
        if (i < 0 || i >= consumerCursors.length) throw new IndexOutOfBoundsException(Integer.toString(i));
        return new Consumer(i);
    }

    /** Convenience: build all {@code consumerCount} consumers. */
    public List<Consumer> consumers() {
        List<Consumer> out = new ArrayList<>(consumerCursors.length);
        for (int i = 0; i < consumerCursors.length; i++) out.add(new Consumer(i));
        return out;
    }

    /** Multi-producer side. */
    public final class Producer {
        /**
         * Try to publish a value. Returns {@code false} when the slowest
         * consumer hasn't caught up; the caller should retry or back off.
         */
        public boolean tryPublish(T value) {
            if (value == null) throw new NullPointerException();
            for (;;) {
                long cur = producerCursor.get();
                long next = cur + 1;

                // Slot we'd claim must not still be held by the slowest consumer's prior round.
                long minConsumer = Long.MAX_VALUE;
                for (AtomicLong c : consumerCursors) {
                    long v = c.get();
                    if (v < minConsumer) minConsumer = v;
                }
                if (next - minConsumer > capacity) {
                    return false;
                }

                if (!producerCursor.compareAndSet(cur, next)) {
                    Thread.onSpinWait();
                    continue;
                }

                int idx = (int) (next & mask);
                long prevRound = next - capacity;
                if (prevRound >= 0) {
                    // Wait for consumers to pass the prior round so we don't
                    // clobber a slot mid-read.
                    outer:
                    while (true) {
                        for (AtomicLong c : consumerCursors) {
                            if (c.get() < prevRound) {
                                Thread.onSpinWait();
                                continue outer;
                            }
                        }
                        break;
                    }
                }

                buf[idx] = value;
                // Publish: store our absolute sequence number.
                published[idx].set(next);
                return true;
            }
        }
    }

    /** One consumer side. Each consumer sees every published item. */
    public final class Consumer {
        private final int idx;
        private long next;

        Consumer(int i) {
            this.idx = i;
            this.next = 0;
        }

        /** Returns the next published item, or {@code null} if not ready. */
        @SuppressWarnings("unchecked")
        public T tryConsume() {
            long seq = next;
            int slot = (int) (seq & mask);
            if (published[slot].get() != seq) return null;
            T v = (T) buf[slot];
            next = seq + 1;
            consumerCursors[idx].set(seq);
            return v;
        }
    }
}
