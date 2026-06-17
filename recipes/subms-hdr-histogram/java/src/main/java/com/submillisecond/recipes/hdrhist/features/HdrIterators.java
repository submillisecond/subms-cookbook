package com.submillisecond.recipes.hdrhist.features;

import com.submillisecond.recipes.hdrhist.HdrHistogram;
import java.util.Iterator;
import java.util.NoSuchElementException;

/**
 * Explicit iterators over a base {@link HdrHistogram} in different orders.
 *
 * <ul>
 *   <li><b>Linear</b>: every populated bucket in value order, one entry each.</li>
 *   <li><b>Logarithmic</b>: bucket boundaries aligned to powers of two.
 *       Each step yields the sum of counts in the half-open band
 *       {@code [lo, hi)}.</li>
 *   <li><b>Percentile</b>: yields buckets at evenly-spaced percentile
 *       thresholds. Caller picks the step (1.0 -> ~100 entries, 0.1 -> ~1000).</li>
 * </ul>
 *
 * <p>Zero-allocation after construction: each iterator holds a reference
 * to the histogram plus a small cursor.
 */
public final class HdrIterators {

    private HdrIterators() {}

    public static Iterator<IterEntry> linear(HdrHistogram h) {
        return new LinearIter(h);
    }

    public static Iterator<IterEntry> logarithmic(HdrHistogram h) {
        return new LogarithmicIter(h);
    }

    public static Iterator<IterEntry> percentiles(HdrHistogram h, double stepPercent) {
        return new PercentileIter(h, stepPercent);
    }

    // ----- linear -----

    static final class LinearIter implements Iterator<IterEntry> {
        private final HdrHistogram h;
        private final long[] counters;
        private final int end;
        private int idx;
        private long cumulative;
        private IterEntry pending;

        LinearIter(HdrHistogram h) {
            this.h = h;
            this.counters = h.counters();
            this.end = Math.min(h.highIndex() + 1, counters.length);
            this.idx = 0;
        }

        private void advance() {
            if (pending != null) return;
            while (idx < end) {
                int i = idx++;
                long c = counters[i];
                if (c == 0L) continue;
                long lo = h.valueFromIndex(i);
                long hi = h.valueFromIndex(i + 1);
                cumulative += c;
                pending = new IterEntry(lo, hi, c, cumulative);
                return;
            }
        }

        @Override public boolean hasNext() {
            advance();
            return pending != null;
        }

        @Override public IterEntry next() {
            advance();
            if (pending == null) throw new NoSuchElementException();
            IterEntry e = pending;
            pending = null;
            return e;
        }
    }

    // ----- logarithmic -----

    static final class LogarithmicIter implements Iterator<IterEntry> {
        private final HdrHistogram h;
        private final long[] counters;
        private final int end;
        private final long highValue;
        private long lo = 1L;
        private long cumulative;
        private boolean done;
        private IterEntry pending;

        LogarithmicIter(HdrHistogram h) {
            this.h = h;
            this.counters = h.counters();
            this.end = Math.min(h.highIndex() + 1, counters.length);
            this.highValue = h.valueFromIndex(h.highIndex());
        }

        private void advance() {
            if (pending != null || done) return;
            long hi = (lo > (Long.MAX_VALUE / 2)) ? Long.MAX_VALUE : lo * 2;
            long count = 0;
            for (int i = 0; i < end; i++) {
                long v = h.valueFromIndex(i);
                if (v >= lo && v < hi) count += counters[i];
            }
            cumulative += count;
            IterEntry e = new IterEntry(lo, hi, count, cumulative);
            if (hi > highValue) done = true;
            lo = hi;
            pending = e;
        }

        @Override public boolean hasNext() {
            advance();
            return pending != null;
        }

        @Override public IterEntry next() {
            advance();
            if (pending == null) throw new NoSuchElementException();
            IterEntry e = pending;
            pending = null;
            return e;
        }
    }

    // ----- percentile -----

    static final class PercentileIter implements Iterator<IterEntry> {
        private final HdrHistogram h;
        private final long[] counters;
        private final int end;
        private final long total;
        private final double stepPct;
        private double nextPct;
        private int idx;
        private long cum;
        private IterEntry pending;

        PercentileIter(HdrHistogram h, double stepPercent) {
            this.h = h;
            this.counters = h.counters();
            this.end = Math.min(h.highIndex() + 1, counters.length);
            this.total = h.count();
            double step = Math.max(Double.MIN_VALUE, stepPercent);
            this.stepPct = step;
            this.nextPct = step;
        }

        private void advance() {
            if (pending != null) return;
            if (total == 0L || nextPct > 100.0 + 1e-9) return;
            long target = (long) ((nextPct / 100.0) * total);
            while (idx < end) {
                cum += counters[idx];
                if (cum >= target) {
                    long lo = h.valueFromIndex(idx);
                    long hi = h.valueFromIndex(idx + 1);
                    long count = counters[idx];
                    double pctNow = nextPct;
                    nextPct += stepPct;
                    long cumulative = Math.max(cum, (long) ((pctNow / 100.0) * total));
                    cumulative = Math.min(cumulative, total);
                    pending = new IterEntry(lo, hi, count, cumulative);
                    return;
                }
                idx++;
            }
        }

        @Override public boolean hasNext() {
            advance();
            return pending != null;
        }

        @Override public IterEntry next() {
            advance();
            if (pending == null) throw new NoSuchElementException();
            IterEntry e = pending;
            pending = null;
            return e;
        }
    }
}
