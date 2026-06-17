package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;

import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.concurrent.locks.LockSupport;

/**
 * Pluggable wait strategies for blocking wrappers around the base wait-free
 * SPSC ring. The base ring returns immediately on full / empty; these
 * wrappers block until a slot is available using one of:
 *
 * <ul>
 *   <li>{@link BusySpin}: tight {@code Thread.onSpinWait()}; lowest wakeup
 *       latency, highest CPU.
 *   <li>{@link YieldStrategy}: {@link Thread#yield()} between retries; good
 *       default when threads share cores with other work.
 *   <li>{@link ParkStrategy}: {@link LockSupport#park} / unpark; lowest CPU,
 *       adds a few microseconds of wakeup latency. Build a matched pair via
 *       {@link ParkStrategy#pair()}.
 * </ul>
 */
public final class WaitStrategies {
    private WaitStrategies() {}

    /** Strategy contract. {@code wait} blocks; {@code signal} wakes the opposite side. */
    public interface WaitStrategy {
        void waitOnce();
        default void signal() {}
    }

    /** Tight {@code Thread.onSpinWait()} spin. */
    public static final class BusySpin implements WaitStrategy {
        @Override public void waitOnce() { Thread.onSpinWait(); }
    }

    /** {@code Thread.yield()} between retries. */
    public static final class YieldStrategy implements WaitStrategy {
        @Override public void waitOnce() { Thread.yield(); }
    }

    /**
     * {@link LockSupport#park} / unpark. The producer-side and consumer-side
     * strategies share a {@link Parker} so each end can wake the other.
     * Use {@link #pair()} to construct.
     */
    public static final class ParkStrategy implements WaitStrategy {
        private final Parker parker;
        private final boolean isProducer;

        private ParkStrategy(Parker p, boolean producer) {
            this.parker = p;
            this.isProducer = producer;
        }

        /** Build a matched (producer strategy, consumer strategy). */
        public static ParkStrategy[] pair() {
            Parker p = new Parker();
            return new ParkStrategy[]{new ParkStrategy(p, true), new ParkStrategy(p, false)};
        }

        @Override
        public void waitOnce() {
            if (isProducer) parker.parkProducer();
            else parker.parkConsumer();
        }

        @Override
        public void signal() {
            // Wake the OPPOSITE end.
            if (isProducer) parker.unparkConsumer();
            else parker.unparkProducer();
        }
    }

    /** Shared park state. Defeats lost-wakeups via a per-side unparked flag. */
    private static final class Parker {
        private final AtomicReference<Thread> producer = new AtomicReference<>();
        private final AtomicReference<Thread> consumer = new AtomicReference<>();
        private final AtomicBoolean producerUnparked = new AtomicBoolean();
        private final AtomicBoolean consumerUnparked = new AtomicBoolean();

        void parkProducer() {
            if (producerUnparked.getAndSet(false)) return;
            producer.set(Thread.currentThread());
            if (producerUnparked.getAndSet(false)) {
                producer.set(null);
                return;
            }
            LockSupport.park(this);
            producer.set(null);
            producerUnparked.set(false);
        }

        void parkConsumer() {
            if (consumerUnparked.getAndSet(false)) return;
            consumer.set(Thread.currentThread());
            if (consumerUnparked.getAndSet(false)) {
                consumer.set(null);
                return;
            }
            LockSupport.park(this);
            consumer.set(null);
            consumerUnparked.set(false);
        }

        void unparkProducer() {
            producerUnparked.set(true);
            Thread t = producer.get();
            if (t != null) LockSupport.unpark(t);
        }

        void unparkConsumer() {
            consumerUnparked.set(true);
            Thread t = consumer.get();
            if (t != null) LockSupport.unpark(t);
        }
    }

    /** Blocking SPSC producer: {@link #push} waits using the supplied strategy. */
    public static final class BlockingSpscProducer<T> {
        private final SpscRingBuffer<T>.Producer inner;
        private final WaitStrategy strategy;

        public BlockingSpscProducer(SpscRingBuffer<T>.Producer producer, WaitStrategy strategy) {
            this.inner = producer;
            this.strategy = strategy;
        }

        /** Block until the value can be pushed. */
        public void push(T value) {
            while (!inner.tryPush(value)) {
                strategy.waitOnce();
            }
            strategy.signal();
        }

        public boolean tryPush(T value) {
            boolean r = inner.tryPush(value);
            if (r) strategy.signal();
            return r;
        }
    }

    /** Blocking SPSC consumer: {@link #pop} waits using the supplied strategy. */
    public static final class BlockingSpscConsumer<T> {
        private final SpscRingBuffer<T>.Consumer inner;
        private final WaitStrategy strategy;

        public BlockingSpscConsumer(SpscRingBuffer<T>.Consumer consumer, WaitStrategy strategy) {
            this.inner = consumer;
            this.strategy = strategy;
        }

        /** Block until an item is available. */
        public T pop() {
            T v;
            while ((v = inner.tryPop()) == null) {
                strategy.waitOnce();
            }
            strategy.signal();
            return v;
        }

        public T tryPop() {
            T v = inner.tryPop();
            if (v != null) strategy.signal();
            return v;
        }
    }
}
