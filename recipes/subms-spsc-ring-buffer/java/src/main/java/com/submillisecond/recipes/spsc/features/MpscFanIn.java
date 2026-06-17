package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;

import java.util.ArrayList;
import java.util.List;

/**
 * N-producer single-consumer fan-in over N independent SPSC rings.
 *
 * <p>Each producer pushes into its own SPSC ring (so it stays wait-free
 * against its own counter), and the consumer round-robins across all rings.
 * The consumer never blocks any producer; producers never contend with each
 * other.
 *
 * <p>Memory cost: {@code N * capacity} slots. Round-robin is fair under
 * steady-state load; under skewed load the consumer spends extra
 * {@code tryPop} calls on quiet producers but never starves a busy one.
 */
public final class MpscFanIn<T> {

    private final List<SpscRingBuffer<T>> rings;
    private final List<SpscRingBuffer<T>.Producer> producers;
    private final List<SpscRingBuffer<T>.Consumer> consumers;
    private final Consumer consumer;

    /** Build with {@code producerCount} rings of capacity {@code perRingCapacity}. */
    public MpscFanIn(int producerCount, int perRingCapacity) {
        if (producerCount < 1) throw new IllegalArgumentException("need at least one producer");
        this.rings = new ArrayList<>(producerCount);
        this.producers = new ArrayList<>(producerCount);
        this.consumers = new ArrayList<>(producerCount);
        for (int i = 0; i < producerCount; i++) {
            SpscRingBuffer<T> r = new SpscRingBuffer<>(perRingCapacity);
            rings.add(r);
            producers.add(r.producer());
            consumers.add(r.consumer());
        }
        this.consumer = new Consumer();
    }

    public int producerCount() { return rings.size(); }

    /** Producer handle at index {@code i}. Move it to its owning thread. */
    public Producer producer(int i) { return new Producer(producers.get(i)); }

    /** The single consumer that round-robins across all producer rings. */
    public Consumer consumer() { return consumer; }

    /** One producer side. Wait-free against its own ring. */
    public final class Producer {
        private final SpscRingBuffer<T>.Producer inner;
        private Producer(SpscRingBuffer<T>.Producer inner) { this.inner = inner; }

        public boolean tryPush(T value) { return inner.tryPush(value); }
    }

    /** The consumer side. Round-robins across producer rings. */
    public final class Consumer {
        private int cursor;

        /**
         * Probe each producer ring once starting at the current cursor. Returns
         * the first value found, advancing the cursor past the producing ring
         * so the next call starts at a fresh point.
         */
        public T tryPop() {
            int n = consumers.size();
            for (int offset = 0; offset < n; offset++) {
                int idx = (cursor + offset) % n;
                T v = consumers.get(idx).tryPop();
                if (v != null) {
                    cursor = (idx + 1) % n;
                    return v;
                }
            }
            return null;
        }

        public int producerCount() { return consumers.size(); }
    }
}
