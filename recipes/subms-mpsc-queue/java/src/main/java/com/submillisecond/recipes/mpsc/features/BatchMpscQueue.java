package com.submillisecond.recipes.mpsc.features;

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
}
