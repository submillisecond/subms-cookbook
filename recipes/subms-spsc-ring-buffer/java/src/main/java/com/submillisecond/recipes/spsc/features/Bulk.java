package com.submillisecond.recipes.spsc.features;

import com.submillisecond.recipes.spsc.SpscRingBuffer;

/**
 * Bulk-transfer entry points for the SPSC ring.
 *
 * <p>The actual bulk methods live on {@link SpscRingBuffer.Producer#tryPushBulk}
 * and {@link SpscRingBuffer.Consumer#tryPopBulk} so they share the producer
 * and consumer cached state with the single-item methods. This class is the
 * documentation home for the {@code bulk} feature and a tidy static facade
 * if a caller prefers the function-style API.
 *
 * <p>Both feature methods amortise the per-item atomic cost behind a single
 * Release fence per call. They return the count actually transferred; a
 * return less than {@code values.length} / {@code out.length} means the ring
 * couldn't take the rest right now (full / empty).
 */
public final class Bulk {
    private Bulk() {}

    /** Static facade for {@link SpscRingBuffer.Producer#tryPushBulk}. */
    public static <T> int tryEnqueueBulk(SpscRingBuffer<T>.Producer p, T[] values) {
        return p.tryPushBulk(values);
    }

    /** Static facade for {@link SpscRingBuffer.Consumer#tryPopBulk}. */
    public static <T> int tryDequeueBulk(SpscRingBuffer<T>.Consumer c, T[] out) {
        return c.tryPopBulk(out);
    }
}
