package com.submillisecond.recipes.tscdc;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.spsc.SpscRingBuffer;
import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/**
 * A {@link TsCollection} that publishes its mutations to subscribers.
 *
 * <p>Each {@link #subscribe(int)} hands back a {@link TsSubscription} backed by
 * its own SPSC ring; this collection holds the producer end of every ring. A
 * mutation runs against the inner collection first, then fans the matching
 * {@link TsChangeEvent} out to every ring. A full ring is skipped and counted
 * in {@link #droppedEvents()} - the mutation still succeeds, so a stalled
 * reader never blocks the writer.
 *
 * <p>With zero subscribers the publish path is an empty-list walk: no events
 * are constructed and {@code push} runs at inner-collection speed.
 *
 * @param <T> the series value type
 */
public final class TsObservableCollection<T> {

    private final TsCollection<T> inner = new TsCollection<>();
    private final List<SpscRingBuffer<TsChangeEvent<T>>.Producer> subscribers = new ArrayList<>();
    private long dropped;

    public TsObservableCollection() {}

    /**
     * Register a new subscriber. {@code capacity} is the requested ring depth
     * (rounded up to a power of two by the ring). Returns the read handle; the
     * write end is retained internally.
     */
    public TsSubscription<T> subscribe(int capacity) {
        SpscRingBuffer<TsChangeEvent<T>> ring = new SpscRingBuffer<>(capacity);
        subscribers.add(ring.producer());
        return new TsSubscription<>(ring.consumer());
    }

    /** Number of subscribers with a live ring. */
    public int subscriberCount() {
        return subscribers.size();
    }

    /**
     * Total events dropped across all rings because the target ring was full at
     * publish time. Monotonic for the lifetime of the collection.
     */
    public long droppedEvents() {
        return dropped;
    }

    /** Borrow the inner collection for reads (get, byName, size, ...). */
    public TsCollection<T> collection() {
        return inner;
    }

    private void publish(TsChangeEvent<T> event) {
        if (subscribers.isEmpty()) {
            return;
        }
        long d = 0;
        for (SpscRingBuffer<TsChangeEvent<T>>.Producer tx : subscribers) {
            if (!tx.tryPush(event)) {
                d++;
            }
        }
        dropped += d;
    }

    // ---------- mirrored mutating surface ----------

    /**
     * Register an empty series. No event is published - registration is not a
     * data change, and the consumer learns of the series on its first Push.
     */
    public long register(TsSeriesMetadata meta) {
        return inner.register(meta);
    }

    /** Append a point, then publish a {@link TsChangeEvent.Push}. */
    public void push(long id, long ts, T value) {
        inner.push(id, ts, value);
        publish(new TsChangeEvent.Push<>(id, ts, value));
    }

    /**
     * Delete the point at {@code ts}, then publish a {@link TsChangeEvent.DeleteAt}
     * iff a point was actually removed.
     */
    public Optional<TsPoint<T>> deleteAt(long id, long ts) {
        Optional<TsPoint<T>> removed = inner.deleteAt(id, ts);
        if (removed.isPresent()) {
            publish(new TsChangeEvent.DeleteAt<>(id, ts));
        }
        return removed;
    }

    /**
     * Delete the {@code [lo, hi]} range, then publish a
     * {@link TsChangeEvent.DeleteRange} iff at least one point was removed.
     * Returns the removed count.
     */
    public int deleteRange(long id, long lo, long hi) {
        int n = inner.deleteRange(id, lo, hi);
        if (n > 0) {
            publish(new TsChangeEvent.DeleteRange<>(id, lo, hi));
        }
        return n;
    }

    /**
     * Deregister a series, then publish a {@link TsChangeEvent.Deregister} iff
     * the series existed. Returns the removed series.
     */
    public Optional<TsSeries<T>> deregister(long id) {
        Optional<TsSeries<T>> removed = inner.deregister(id);
        if (removed.isPresent()) {
            publish(new TsChangeEvent.Deregister<>(id));
        }
        return removed;
    }
}
