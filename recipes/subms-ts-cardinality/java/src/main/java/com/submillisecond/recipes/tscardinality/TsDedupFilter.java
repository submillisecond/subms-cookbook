package com.submillisecond.recipes.tscardinality;

import java.util.HashSet;
import java.util.Set;

/**
 * Exact idempotent-ingest filter. The first sight of a {@link TsIngestKey} is
 * new; every later sight of the same key is a replay and is dropped.
 *
 * <p>Backed by an exact {@link HashSet}, so memory grows with the number of
 * distinct keys seen. That is the right call when the dedup window is bounded
 * upstream (a fixed sequence range per series, a per-flush reset). For an
 * unbounded stream a future bounded / rolling-bloom variant trades exactness
 * for a fixed footprint - see the recipe writeup.
 */
public final class TsDedupFilter {

    private final Set<TsIngestKey> seen;

    public TsDedupFilter() {
        this.seen = new HashSet<>();
    }

    public TsDedupFilter(int initialCapacity) {
        this.seen = new HashSet<>(initialCapacity);
    }

    /** True the first time {@code key} is seen (and records it); false on any replay. */
    public boolean isNew(TsIngestKey key) {
        return seen.add(key);
    }

    /** Test without recording. Lets a caller peek before committing the write. */
    public boolean contains(TsIngestKey key) {
        return seen.contains(key);
    }

    public int seenCount() {
        return seen.size();
    }

    public boolean isEmpty() {
        return seen.isEmpty();
    }

    /** Forget every key. Use at a dedup-window boundary (a flush, a new epoch). */
    public void reset() {
        seen.clear();
    }
}
