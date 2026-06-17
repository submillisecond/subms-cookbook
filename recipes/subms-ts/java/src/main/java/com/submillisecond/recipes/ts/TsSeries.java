package com.submillisecond.recipes.ts;

import java.time.Instant;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Optional;
import java.util.function.Predicate;

/**
 * A time-ordered sequence of {@link TsPoint}. Non-decreasing in {@code ts};
 * out-of-order or null inserts are rejected by {@link #push}.
 *
 * <p>Backed by a mutable SoA head chunk plus a list of sealed warm chunks. A
 * series under {@link #SEAL_CAP} points lives entirely in the head. This is
 * the general (boxed) path; {@link TsSeriesD} / {@link TsSeriesL} keep
 * primitive columns for the numeric fast path.
 */
public final class TsSeries<T> implements Iterable<TsPoint<T>> {

    static final int SEAL_CAP = 65_536;

    private static final class Chunk<T> {
        long[] ts;
        final List<T> val;
        int len;

        Chunk(int cap) {
            this.ts = new long[Math.max(1, cap)];
            this.val = new ArrayList<>(Math.max(1, cap));
            this.len = 0;
        }

        boolean isEmpty() {
            return len == 0;
        }

        long tsMin() {
            return ts[0];
        }

        long tsMax() {
            return ts[len - 1];
        }

        void append(long t, T v) {
            if (len == ts.length) {
                long[] grown = new long[ts.length * 2];
                System.arraycopy(ts, 0, grown, 0, len);
                ts = grown;
            }
            ts[len] = t;
            val.add(v);
            len++;
        }

        // First index whose ts >= target.
        int firstGe(long target) {
            int lo = 0;
            int hi = len;
            while (lo < hi) {
                int mid = (lo + hi) >>> 1;
                if (ts[mid] < target) lo = mid + 1;
                else hi = mid;
            }
            return lo;
        }

        // First index whose ts > target.
        int firstGt(long target) {
            int lo = 0;
            int hi = len;
            while (lo < hi) {
                int mid = (lo + hi) >>> 1;
                if (ts[mid] <= target) lo = mid + 1;
                else hi = mid;
            }
            return lo;
        }

        TsPoint<T> point(int i) {
            return new TsPoint<>(ts[i], val.get(i));
        }

        TsPoint<T> removeAt(int i) {
            long t = ts[i];
            T v = val.remove(i);
            System.arraycopy(ts, i + 1, ts, i, len - i - 1);
            len--;
            return new TsPoint<>(t, v);
        }
    }

    private final List<Chunk<T>> warm = new ArrayList<>();
    private Chunk<T> head = new Chunk<>(16);
    private int len = 0;
    private Long lastTs = null;
    private TsSeriesMetadata meta = null;

    public TsSeries() {}

    public static <T> TsSeries<T> withCapacity(int cap) {
        TsSeries<T> s = new TsSeries<>();
        s.head = new Chunk<>(Math.min(Math.max(1, cap), SEAL_CAP));
        return s;
    }

    /** Build from points already in non-decreasing ts order. */
    public static <T> TsSeries<T> fromPoints(List<TsPoint<T>> points) {
        TsSeries<T> s = withCapacity(points.size());
        for (TsPoint<T> p : points) {
            s.push(p.ts(), p.value());
        }
        return s;
    }

    public TsSeries<T> withMetadata(TsSeriesMetadata meta) {
        this.meta = meta;
        return this;
    }

    public Optional<TsSeriesMetadata> metadata() {
        return Optional.ofNullable(meta);
    }

    public void setMetadata(TsSeriesMetadata meta) {
        this.meta = meta;
    }

    /**
     * Append an observation. Rejects a ts earlier than the tail
     * ({@link TsException.Kind#NOT_MONOTONIC}) and a null / non-finite value
     * ({@link TsException.Kind#NULL_VALUE}).
     */
    public void push(long ts, T value) {
        if (!isPresent(value)) {
            throw TsException.nullValue("non-finite or null observation");
        }
        if (lastTs != null && ts < lastTs) {
            throw TsException.notMonotonic(lastTs, ts);
        }
        head.append(ts, value);
        len++;
        lastTs = ts;
        if (head.len == SEAL_CAP) {
            seal();
        }
    }

    // Mirrors the Rust TsValueKind impls: scalar floats reject non-finite,
    // TsValueKind types defer to their own check, everything else is present
    // unless null.
    private static boolean isPresent(Object value) {
        if (value == null) return false;
        if (value instanceof Double d) return Double.isFinite(d);
        if (value instanceof Float f) return Float.isFinite(f);
        if (value instanceof TsValueKind k) return k.tsIsPresent();
        return true;
    }

    private void seal() {
        warm.add(head);
        head = new Chunk<>(SEAL_CAP);
    }

    public int size() {
        return len;
    }

    public boolean isEmpty() {
        return len == 0;
    }

    public Optional<TsPoint<T>> first() {
        for (Chunk<T> c : warm) {
            if (!c.isEmpty()) return Optional.of(c.point(0));
        }
        if (!head.isEmpty()) return Optional.of(head.point(0));
        return Optional.empty();
    }

    public Optional<TsPoint<T>> last() {
        if (!head.isEmpty()) return Optional.of(head.point(head.len - 1));
        for (int i = warm.size() - 1; i >= 0; i--) {
            Chunk<T> c = warm.get(i);
            if (!c.isEmpty()) return Optional.of(c.point(c.len - 1));
        }
        return Optional.empty();
    }

    private List<Chunk<T>> nonEmptyChunks() {
        List<Chunk<T>> out = new ArrayList<>(warm.size() + 1);
        for (Chunk<T> c : warm) {
            if (!c.isEmpty()) out.add(c);
        }
        if (!head.isEmpty()) out.add(head);
        return out;
    }

    @Override
    public Iterator<TsPoint<T>> iterator() {
        List<Chunk<T>> chunks = nonEmptyChunks();
        return new Iterator<>() {
            private int ci = 0;
            private int pos = 0;

            @Override
            public boolean hasNext() {
                while (ci < chunks.size()) {
                    if (pos < chunks.get(ci).len) return true;
                    ci++;
                    pos = 0;
                }
                return false;
            }

            @Override
            public TsPoint<T> next() {
                if (!hasNext()) throw new NoSuchElementException();
                return chunks.get(ci).point(pos++);
            }
        };
    }

    // ---------- time queries ----------

    public Optional<TsPoint<T>> getAt(long target) {
        for (Chunk<T> c : nonEmptyChunks()) {
            if (target < c.tsMin()) return Optional.empty();
            if (target > c.tsMax()) continue;
            int i = c.firstGe(target);
            if (i < c.len && c.ts[i] == target) return Optional.of(c.point(i));
        }
        return Optional.empty();
    }

    public Optional<TsPoint<T>> nearestBefore(long target) {
        Chunk<T> best = null;
        for (Chunk<T> c : nonEmptyChunks()) {
            if (c.tsMin() <= target) best = c;
            else break;
        }
        if (best == null) return Optional.empty();
        int gt = best.firstGt(target);
        return gt == 0 ? Optional.empty() : Optional.of(best.point(gt - 1));
    }

    public Optional<TsPoint<T>> nearestAfter(long target) {
        for (Chunk<T> c : nonEmptyChunks()) {
            if (c.tsMax() < target) continue;
            int i = c.firstGe(target);
            if (i < c.len) return Optional.of(c.point(i));
        }
        return Optional.empty();
    }

    public Optional<TsPoint<T>> nearest(long target) {
        Optional<TsPoint<T>> before = nearestBefore(target);
        Optional<TsPoint<T>> after = nearestAfter(target);
        if (before.isPresent() && after.isPresent()) {
            TsPoint<T> b = before.get();
            TsPoint<T> a = after.get();
            return (target - b.ts()) <= (a.ts() - target) ? before : after;
        }
        return before.isPresent() ? before : after;
    }

    /** Inclusive {@code [lo, hi]} range as a lazy view over the chunks. */
    public TsRange<T> range(long lo, long hi) {
        if (lo > hi) return TsRange.empty();
        List<long[]> tsSpans = new ArrayList<>();
        List<List<T>> valSpans = new ArrayList<>();
        for (Chunk<T> c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            if (start < end) {
                long[] ts = new long[end - start];
                System.arraycopy(c.ts, start, ts, 0, end - start);
                tsSpans.add(ts);
                valSpans.add(new ArrayList<>(c.val.subList(start, end)));
            }
        }
        return new TsRange<>(tsSpans, valSpans);
    }

    /** java.time sugar: inclusive range between two instants (nanos). */
    public TsRange<T> rangeInstant(Instant lo, Instant hi) {
        return range(toNanos(lo), toNanos(hi));
    }

    private static long toNanos(Instant when) {
        return Math.addExact(Math.multiplyExact(when.getEpochSecond(), 1_000_000_000L), when.getNano());
    }

    // ---------- delete surface ----------

    public Optional<TsPoint<T>> deleteAt(long target) {
        for (int ci = 0; ci <= warm.size(); ci++) {
            Chunk<T> c = ci < warm.size() ? warm.get(ci) : head;
            if (c.isEmpty() || target < c.tsMin() || target > c.tsMax()) continue;
            int i = c.firstGe(target);
            if (i < c.len && c.ts[i] == target) {
                TsPoint<T> removed = c.removeAt(i);
                len--;
                dropEmptyWarm();
                recomputeLastTs();
                return Optional.of(removed);
            }
        }
        return Optional.empty();
    }

    public int deleteRange(long lo, long hi) {
        if (lo > hi) return 0;
        return retainPoints((ts, v) -> ts < lo || ts > hi);
    }

    public int deleteByValue(T target) {
        return retainPoints((ts, v) -> !java.util.Objects.equals(v, target));
    }

    public int deleteValueRange(T lo, T hi, java.util.Comparator<T> cmp) {
        return retainPoints((ts, v) -> cmp.compare(v, lo) < 0 || cmp.compare(v, hi) > 0);
    }

    public int retain(Predicate<TsPoint<T>> keep) {
        return retainPoints((ts, v) -> keep.test(new TsPoint<>(ts, v)));
    }

    public int truncateBefore(long cutoff) {
        return retainPoints((ts, v) -> ts >= cutoff);
    }

    public int truncateAfter(long cutoff) {
        return retainPoints((ts, v) -> ts <= cutoff);
    }

    public Optional<TsPoint<T>> popFirst() {
        return first().flatMap(p -> deleteAt(p.ts()));
    }

    public Optional<TsPoint<T>> popLast() {
        Optional<TsPoint<T>> lastOpt = last();
        if (lastOpt.isEmpty()) return Optional.empty();
        Chunk<T> c = !head.isEmpty() ? head : warm.get(warm.size() - 1);
        TsPoint<T> removed = c.removeAt(c.len - 1);
        len--;
        dropEmptyWarm();
        recomputeLastTs();
        return Optional.of(removed);
    }

    public void clear() {
        warm.clear();
        head = new Chunk<>(16);
        len = 0;
        lastTs = null;
    }

    private interface KeepFn<T> {
        boolean keep(long ts, T v);
    }

    private int retainPoints(KeepFn<T> keep) {
        int before = len;
        List<Chunk<T>> newWarm = new ArrayList<>();
        Chunk<T> cur = new Chunk<>(SEAL_CAP);
        int kept = 0;
        List<Chunk<T>> all = new ArrayList<>(warm);
        all.add(head);
        warm.clear();
        head = new Chunk<>(16);
        for (Chunk<T> c : all) {
            for (int i = 0; i < c.len; i++) {
                if (keep.keep(c.ts[i], c.val.get(i))) {
                    cur.append(c.ts[i], c.val.get(i));
                    kept++;
                    if (cur.len == SEAL_CAP) {
                        newWarm.add(cur);
                        cur = new Chunk<>(SEAL_CAP);
                    }
                }
            }
        }
        warm.addAll(newWarm);
        head = cur;
        len = kept;
        recomputeLastTs();
        return before - kept;
    }

    private void dropEmptyWarm() {
        warm.removeIf(Chunk::isEmpty);
    }

    private void recomputeLastTs() {
        lastTs = last().map(TsPoint::ts).orElse(null);
    }

    // ---------- numeric surface (operator-bundle gated) ----------

    public Optional<T> min(TsNumeric<T> num) {
        T best = null;
        for (TsPoint<T> p : this) {
            if (best == null || num.compare(p.value(), best) < 0) best = p.value();
        }
        return Optional.ofNullable(best);
    }

    public Optional<T> max(TsNumeric<T> num) {
        T best = null;
        for (TsPoint<T> p : this) {
            if (best == null || num.compare(p.value(), best) > 0) best = p.value();
        }
        return Optional.ofNullable(best);
    }

    public T sum(TsNumeric<T> num) {
        T acc = num.zero();
        for (TsPoint<T> p : this) acc = num.add(acc, p.value());
        return acc;
    }

    public Optional<Double> mean(TsNumeric<T> num) {
        if (isEmpty()) return Optional.empty();
        return Optional.of(num.toDouble(sum(num)) / len);
    }

    public Optional<TsPoint<T>> minPoint(TsNumeric<T> num) {
        TsPoint<T> best = null;
        for (TsPoint<T> p : this) {
            if (best == null || num.compare(p.value(), best.value()) < 0) best = p;
        }
        return Optional.ofNullable(best);
    }

    public Optional<TsPoint<T>> maxPoint(TsNumeric<T> num) {
        TsPoint<T> best = null;
        for (TsPoint<T> p : this) {
            if (best == null || num.compare(p.value(), best.value()) > 0) best = p;
        }
        return Optional.ofNullable(best);
    }

    public Optional<T> rangeMin(long lo, long hi, TsNumeric<T> num) {
        T best = null;
        for (TsPoint<T> p : range(lo, hi)) {
            if (best == null || num.compare(p.value(), best) < 0) best = p.value();
        }
        return Optional.ofNullable(best);
    }

    public Optional<T> rangeMax(long lo, long hi, TsNumeric<T> num) {
        T best = null;
        for (TsPoint<T> p : range(lo, hi)) {
            if (best == null || num.compare(p.value(), best) > 0) best = p.value();
        }
        return Optional.ofNullable(best);
    }

    public T rangeSum(long lo, long hi, TsNumeric<T> num) {
        T acc = num.zero();
        for (TsPoint<T> p : range(lo, hi)) acc = num.add(acc, p.value());
        return acc;
    }

    public Optional<Double> rangeMean(long lo, long hi, TsNumeric<T> num) {
        int count = 0;
        T acc = num.zero();
        for (TsPoint<T> p : range(lo, hi)) {
            acc = num.add(acc, p.value());
            count++;
        }
        return count == 0 ? Optional.empty() : Optional.of(num.toDouble(acc) / count);
    }
}
