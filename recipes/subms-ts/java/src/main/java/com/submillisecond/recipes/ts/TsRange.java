package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;

/**
 * Lazy inclusive-range view over a {@link TsSeries}. Borrows the underlying
 * chunk columns and yields {@link TsPoint} without materialising an
 * intermediate list. The opaque view is deliberate: a future SIMD / cold-tier
 * pass swaps the scan strategy underneath without changing the signature.
 */
public final class TsRange<T> implements Iterable<TsPoint<T>> {

    private final List<long[]> tsSpans;
    private final List<List<T>> valSpans;

    TsRange(List<long[]> tsSpans, List<List<T>> valSpans) {
        this.tsSpans = tsSpans;
        this.valSpans = valSpans;
    }

    static <T> TsRange<T> empty() {
        return new TsRange<>(new ArrayList<>(), new ArrayList<>());
    }

    @Override
    public Iterator<TsPoint<T>> iterator() {
        return new Iterator<>() {
            private int chunk = 0;
            private int pos = 0;

            @Override
            public boolean hasNext() {
                while (chunk < tsSpans.size()) {
                    if (pos < tsSpans.get(chunk).length) return true;
                    chunk++;
                    pos = 0;
                }
                return false;
            }

            @Override
            public TsPoint<T> next() {
                if (!hasNext()) throw new NoSuchElementException();
                TsPoint<T> p = new TsPoint<>(tsSpans.get(chunk)[pos], valSpans.get(chunk).get(pos));
                pos++;
                return p;
            }
        };
    }
}
