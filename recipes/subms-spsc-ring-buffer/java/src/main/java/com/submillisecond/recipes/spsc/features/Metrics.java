package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;

import java.util.concurrent.atomic.AtomicLong;

/**
 * Per-instance metrics wrapper for the SPSC ring.
 *
 * <p>Wraps a base {@link SpscRingBuffer.Producer} / {@link SpscRingBuffer.Consumer}
 * pair and counts:
 *
 * <ul>
 *   <li>{@code enqueueSuccess}, {@code enqueueFail} (ring full)
 *   <li>{@code dequeueSuccess}, {@code dequeueFail} (ring empty)
 *   <li>{@code maxDepthObserved} (gauge; high-water mark of in-flight items)
 *   <li>{@code casRetries} (only the {@link MpmcDisruptor} path bumps this;
 *       SPSC never CAS-retries under the wait-free invariant)
 * </ul>
 *
 * <p>Counter overhead is one {@code AtomicLong} increment per op - measurable
 * on a hot loop, so reach for this when you're capturing operational stats,
 * not when you need the absolute lowest latency.
 */
public final class Metrics {

    /** Shared counter set. Hold across producer + consumer. */
    public static final class RingMetrics {
        private final AtomicLong enqueueSuccess = new AtomicLong();
        private final AtomicLong enqueueFail = new AtomicLong();
        private final AtomicLong dequeueSuccess = new AtomicLong();
        private final AtomicLong dequeueFail = new AtomicLong();
        private final AtomicLong maxDepthObserved = new AtomicLong();
        private final AtomicLong casRetries = new AtomicLong();

        /** Bump the CAS-retry counter. Called by the disruptor path. */
        public void recordCasRetry() { casRetries.incrementAndGet(); }

        /** Capture a point-in-time snapshot. */
        public RingMetricsSnapshot snapshot() {
            return new RingMetricsSnapshot(
                    enqueueSuccess.get(),
                    enqueueFail.get(),
                    dequeueSuccess.get(),
                    dequeueFail.get(),
                    maxDepthObserved.get(),
                    casRetries.get());
        }

        void observeDepth(long depth) {
            long cur;
            do {
                cur = maxDepthObserved.get();
                if (depth <= cur) return;
            } while (!maxDepthObserved.compareAndSet(cur, depth));
        }
    }

    /** Immutable snapshot. */
    public static final class RingMetricsSnapshot {
        public final long enqueueSuccess, enqueueFail;
        public final long dequeueSuccess, dequeueFail;
        public final long maxDepthObserved;
        public final long casRetries;

        public RingMetricsSnapshot(long es, long ef, long ds, long df, long md, long cr) {
            this.enqueueSuccess = es;
            this.enqueueFail = ef;
            this.dequeueSuccess = ds;
            this.dequeueFail = df;
            this.maxDepthObserved = md;
            this.casRetries = cr;
        }
    }

    /** Wrap a base (Producer, Consumer) pair with counters. */
    public static final class Instrumented<T> {
        public final InstrumentedProducer<T> producer;
        public final InstrumentedConsumer<T> consumer;
        public final RingMetrics metrics;

        public Instrumented(SpscRingBuffer<T>.Producer p, SpscRingBuffer<T>.Consumer c) {
            this.metrics = new RingMetrics();
            this.producer = new InstrumentedProducer<>(p, metrics);
            this.consumer = new InstrumentedConsumer<>(c, metrics);
        }
    }

    public static final class InstrumentedProducer<T> {
        private final SpscRingBuffer<T>.Producer inner;
        private final RingMetrics metrics;
        private long localDepth;

        InstrumentedProducer(SpscRingBuffer<T>.Producer inner, RingMetrics metrics) {
            this.inner = inner;
            this.metrics = metrics;
        }

        public boolean tryPush(T value) {
            if (inner.tryPush(value)) {
                metrics.enqueueSuccess.incrementAndGet();
                localDepth++;
                metrics.observeDepth(localDepth);
                return true;
            }
            metrics.enqueueFail.incrementAndGet();
            return false;
        }
    }

    public static final class InstrumentedConsumer<T> {
        private final SpscRingBuffer<T>.Consumer inner;
        private final RingMetrics metrics;

        InstrumentedConsumer(SpscRingBuffer<T>.Consumer inner, RingMetrics metrics) {
            this.inner = inner;
            this.metrics = metrics;
        }

        public T tryPop() {
            T v = inner.tryPop();
            if (v != null) {
                metrics.dequeueSuccess.incrementAndGet();
            } else {
                metrics.dequeueFail.incrementAndGet();
            }
            return v;
        }
    }

    private Metrics() {}
}
