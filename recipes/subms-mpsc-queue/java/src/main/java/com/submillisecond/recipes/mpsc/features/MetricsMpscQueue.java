package com.submillisecond.recipes.mpsc.features;

import java.util.concurrent.atomic.AtomicLong;

import com.submillisecond.recipes.mpsc.MpscQueue;

/**
 * Per-instance metrics wrapper.
 *
 * <p>Wraps the base {@link MpscQueue} with relaxed atomic counters
 * for enqueue success/fail, dequeue success/fail, total batch items
 * drained, and CAS retries.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_mpsc_queue::MetricsMpscQueue}.
 *
 * @param <T> element type; nulls are not permitted.
 */
public final class MetricsMpscQueue<T> {

    private final MpscQueue<T> inner = new MpscQueue<>();
    private final AtomicLong enqueueOk = new AtomicLong();
    private final AtomicLong enqueueFail = new AtomicLong();
    private final AtomicLong dequeueOk = new AtomicLong();
    private final AtomicLong dequeueFail = new AtomicLong();
    private final AtomicLong batchItems = new AtomicLong();
    private final AtomicLong casRetries = new AtomicLong();

    /** Push always succeeds for the unbounded base. */
    public void push(T value) {
        inner.push(value);
        enqueueOk.incrementAndGet();
    }

    /** Single-consumer pop. Bumps {@code dequeueOk} or {@code dequeueFail}. */
    public T tryPoll() {
        T v = inner.tryPoll();
        if (v != null) {
            dequeueOk.incrementAndGet();
        } else {
            dequeueFail.incrementAndGet();
        }
        return v;
    }

    /** Drain up to {@code out.length} items; returns count and bumps {@code batchItems}. */
    public int tryPollBatch(T[] out) {
        int n = 0;
        while (n < out.length) {
            T v = inner.tryPoll();
            if (v == null) {
                dequeueFail.incrementAndGet();
                break;
            }
            out[n++] = v;
            dequeueOk.incrementAndGet();
        }
        batchItems.addAndGet(n);
        return n;
    }

    /**
     * Borrow the next value without consuming it. Does not touch the
     * counters: a peek is not a dequeue.
     */
    public T peek() {
        return inner.peek();
    }

    /** See {@link MpscQueue#isEmpty()}. */
    public boolean isEmpty() {
        return inner.isEmpty();
    }

    /** See {@link MpscQueue#size()}. O(n) in the backlog. */
    public int size() {
        return inner.size();
    }

    /**
     * Drain everything reachable and return the count. The drained items count
     * as successful dequeues, so a cleared backlog still shows up in the
     * snapshot rather than vanishing from the totals.
     */
    public int clear() {
        int n = inner.clear();
        dequeueOk.addAndGet(n);
        return n;
    }

    /** External hook for bounded-upstream compositions. */
    public void recordEnqueueFail() {
        enqueueFail.incrementAndGet();
    }

    /** External hook used by MPMC compositions to log retry counts. */
    public void recordCasRetries(long n) {
        if (n > 0) casRetries.addAndGet(n);
    }

    public Snapshot snapshot() {
        return new Snapshot(
            enqueueOk.get(),
            enqueueFail.get(),
            dequeueOk.get(),
            dequeueFail.get(),
            batchItems.get(),
            casRetries.get()
        );
    }

    public void reset() {
        enqueueOk.set(0);
        enqueueFail.set(0);
        dequeueOk.set(0);
        dequeueFail.set(0);
        batchItems.set(0);
        casRetries.set(0);
    }

    /** Immutable point-in-time view of the counters. */
    public static final class Snapshot {
        public final long enqueueOk;
        public final long enqueueFail;
        public final long dequeueOk;
        public final long dequeueFail;
        public final long batchItems;
        public final long casRetries;

        Snapshot(long eo, long ef, long dqo, long df, long b, long c) {
            this.enqueueOk = eo;
            this.enqueueFail = ef;
            this.dequeueOk = dqo;
            this.dequeueFail = df;
            this.batchItems = b;
            this.casRetries = c;
        }

        @Override
        public String toString() {
            return "Snapshot{enqueueOk=" + enqueueOk
                + ", enqueueFail=" + enqueueFail
                + ", dequeueOk=" + dequeueOk
                + ", dequeueFail=" + dequeueFail
                + ", batchItems=" + batchItems
                + ", casRetries=" + casRetries
                + '}';
        }
    }
}
