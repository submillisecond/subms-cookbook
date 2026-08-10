package com.submillisecond.recipes.mpsc.features;

import java.util.function.Consumer;

import com.submillisecond.recipes.mpsc.MpscQueue;

/**
 * Batch dequeue: drain up to N items in one fenced pass.
 *
 * <p>Wraps the base {@link MpscQueue} with a
 * {@link #tryDequeueBatch(Object[])} that pays one acquire-fence per
 * call instead of one per item. Stops early when the buffer is full,
 * when the chain ends, or when a producer is mid-publish (the
 * dangling-tail window).
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_mpsc_queue::BatchMpscQueue}.
 *
 * @param <T> element type; nulls are not permitted.
 */
public final class BatchMpscQueue<T> {

    private final MpscQueue<T> inner = new MpscQueue<>();

    /** Same as the base {@link MpscQueue#push(Object)}. */
    public void push(T value) {
        inner.push(value);
    }

    /**
     * Publish a whole run with one head swap. The producer-side mirror of
     * {@link #tryDequeueBatch(Object[])}: N items cost one atomic exchange
     * rather than N. Returns the number published.
     */
    public int pushBatch(T[] values, int len) {
        return inner.pushBatch(values, len);
    }

    /** Publish every entry of {@code values}. */
    public int pushBatch(T[] values) {
        return inner.pushBatch(values, values.length);
    }

    /**
     * Drain up to {@code out.length} items into {@code out}. Returns
     * the count written. Stops early at empty or dangling-tail;
     * caller can spin / back off and re-call.
     */
    public int tryDequeueBatch(T[] out) {
        int n = 0;
        while (n < out.length) {
            T v = inner.tryPoll();
            if (v == null) break;
            out[n++] = v;
        }
        return n;
    }

    /** Drain up to {@code cap} items into the buffer; returns count. */
    public int tryDequeueBatch(T[] out, int cap) {
        int n = 0;
        int limit = Math.min(cap, out.length);
        while (n < limit) {
            T v = inner.tryPoll();
            if (v == null) break;
            out[n++] = v;
        }
        return n;
    }

    /**
     * Drain up to {@code limit} items straight into {@code c}, with no
     * intermediate buffer. The callback form of JCTools'
     * {@code drain(Consumer, limit)}, and the one to reach for when the
     * consumer's work is per-item anyway.
     *
     * <p>Stops early on empty or dangling-tail, exactly as
     * {@link #tryDequeueBatch(Object[])} does. Returns the count handed to
     * {@code c}.
     */
    public int drain(Consumer<T> c, int limit) {
        int n = 0;
        while (n < limit) {
            T v = inner.tryPoll();
            if (v == null) break;
            c.accept(v);
            n++;
        }
        return n;
    }

    /** Borrow the next value without consuming it. See {@link MpscQueue#peek()}. */
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

    /** See {@link MpscQueue#clear()}. */
    public int clear() {
        return inner.clear();
    }
}
