package com.submillisecond.recipes.ratelimit.features;

import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Objects;

/**
 * Real in-process backend. Holds counters in a
 * {@code HashMap<(key, window), Cell>} guarded by a monitor lock.
 * Garbage-collects expired windows opportunistically on each
 * {@link #incr(String, long, long)} call.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_rate_limiter::InMemoryBackend}.
 */
public final class InMemoryBackend implements Backend {

    private final Map<Key, Cell> counters = new HashMap<>();

    @Override
    public synchronized long incr(String key, long windowStartNs, long ttlNs) {
        // Opportunistic GC against the callsite's clock (matches Rust).
        long now = windowStartNs;
        Iterator<Map.Entry<Key, Cell>> it = counters.entrySet().iterator();
        while (it.hasNext()) {
            Map.Entry<Key, Cell> e = it.next();
            if (e.getValue().expiresNs <= now) it.remove();
        }
        Key k = new Key(key, windowStartNs);
        Cell c = counters.get(k);
        if (c == null) {
            c = new Cell(0L, windowStartNs + ttlNs);
            counters.put(k, c);
        }
        c.count += 1L;
        return c.count;
    }

    @Override
    public synchronized long read(String key, long windowStartNs) {
        Cell c = counters.get(new Key(key, windowStartNs));
        return c == null ? 0L : c.count;
    }

    private static final class Key {
        final String key;
        final long windowStartNs;

        Key(String key, long windowStartNs) {
            this.key = key;
            this.windowStartNs = windowStartNs;
        }

        @Override
        public boolean equals(Object o) {
            if (!(o instanceof Key other)) return false;
            return windowStartNs == other.windowStartNs && Objects.equals(key, other.key);
        }

        @Override
        public int hashCode() {
            return Objects.hash(key, windowStartNs);
        }
    }

    private static final class Cell {
        long count;
        final long expiresNs;

        Cell(long count, long expiresNs) {
            this.count = count;
            this.expiresNs = expiresNs;
        }
    }
}
