package com.submillisecond.recipes.tscardinality;

import java.util.Optional;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsCollectionException;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/**
 * A {@link TsCollection} with a series-count cap bolted on. {@code register}
 * consults the guard before delegating; every read passes straight through.
 * The decorator owns both halves so the guard's count and the collection's
 * series stay in lockstep.
 */
public final class TsGuardedCollection<T> {

    private final TsCollection<T> inner = new TsCollection<>();
    private final TsCardinalityGuard guard;

    public TsGuardedCollection(int maxSeries, TsOverflowPolicy policy) {
        this.guard = new TsCardinalityGuard(maxSeries, policy);
    }

    /**
     * Register a series if the cap allows. The guard decides first; only an
     * accepted admission reaches the collection. A duplicate id / name still
     * fails inside {@link TsCollection}, in which case the borrowed guard slot
     * is released so a rejected register does not silently consume capacity.
     */
    public long register(TsSeriesMetadata meta) {
        guard.admit();
        try {
            return inner.register(meta);
        } catch (TsCollectionException e) {
            guard.release();
            throw e;
        }
    }

    /**
     * Push a point into a registered series. No cardinality decision: a push
     * adds to an existing series, it does not create one. Returns false if the
     * id is unknown.
     */
    public boolean push(long id, long ts, T value) {
        try {
            inner.push(id, ts, value);
            return true;
        } catch (TsCollectionException e) {
            return false;
        }
    }

    public Optional<TsSeries<T>> get(long id) {
        return inner.get(id);
    }

    public Optional<TsSeries<T>> byName(String name) {
        return inner.byName(name);
    }

    /** Deregister a series and free its guard slot. */
    public Optional<TsSeries<T>> deregister(long id) {
        Optional<TsSeries<T>> removed = inner.deregister(id);
        if (removed.isPresent()) guard.release();
        return removed;
    }

    public int size() {
        return inner.size();
    }

    public boolean isEmpty() {
        return inner.isEmpty();
    }

    public int remaining() {
        return guard.remaining();
    }

    public int count() {
        return guard.count();
    }

    /** Borrow the wrapped collection for read-only operations not surfaced here. */
    public TsCollection<T> collection() {
        return inner;
    }
}
