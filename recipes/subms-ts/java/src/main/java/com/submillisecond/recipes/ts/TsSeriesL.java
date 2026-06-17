package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

/**
 * Unboxed scalar-{@code long} time series. Primitive {@code long[]} ts +
 * {@code long[]} value columns - the integer-counter analogue of
 * {@link TsSeriesD}. Independent class; storage is not inherited.
 */
public final class TsSeriesL implements TsNumericSeries {

    static final int SEAL_CAP = 65_536;

    private static final class Chunk {
        long[] ts;
        long[] val;
        int len;

        Chunk(int cap) {
            int c = Math.max(1, cap);
            this.ts = new long[c];
            this.val = new long[c];
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

        void append(long t, long v) {
            if (len == ts.length) {
                long[] gt = new long[ts.length * 2];
                long[] gv = new long[ts.length * 2];
                System.arraycopy(ts, 0, gt, 0, len);
                System.arraycopy(val, 0, gv, 0, len);
                ts = gt;
                val = gv;
            }
            ts[len] = t;
            val[len] = v;
            len++;
        }

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

        TsPoint<Long> point(int i) {
            return new TsPoint<>(ts[i], val[i]);
        }
    }

    private final List<Chunk> warm = new ArrayList<>();
    private Chunk head = new Chunk(16);
    private int len = 0;
    private Long lastTs = null;

    public TsSeriesL() {}

    public static TsSeriesL withCapacity(int cap) {
        TsSeriesL s = new TsSeriesL();
        s.head = new Chunk(Math.min(Math.max(1, cap), SEAL_CAP));
        return s;
    }

    public void push(long ts, long value) {
        if (lastTs != null && ts < lastTs) {
            throw TsException.notMonotonic(lastTs, ts);
        }
        head.append(ts, value);
        len++;
        lastTs = ts;
        if (head.len == SEAL_CAP) {
            warm.add(head);
            head = new Chunk(SEAL_CAP);
        }
    }

    @Override
    public int size() {
        return len;
    }

    @Override
    public boolean isEmpty() {
        return len == 0;
    }

    public Optional<TsPoint<Long>> first() {
        for (Chunk c : warm) {
            if (!c.isEmpty()) return Optional.of(c.point(0));
        }
        return head.isEmpty() ? Optional.empty() : Optional.of(head.point(0));
    }

    public Optional<TsPoint<Long>> last() {
        if (!head.isEmpty()) return Optional.of(head.point(head.len - 1));
        for (int i = warm.size() - 1; i >= 0; i--) {
            Chunk c = warm.get(i);
            if (!c.isEmpty()) return Optional.of(c.point(c.len - 1));
        }
        return Optional.empty();
    }

    private List<Chunk> nonEmptyChunks() {
        List<Chunk> out = new ArrayList<>(warm.size() + 1);
        for (Chunk c : warm) {
            if (!c.isEmpty()) out.add(c);
        }
        if (!head.isEmpty()) out.add(head);
        return out;
    }

    public List<TsPoint<Long>> toList() {
        List<TsPoint<Long>> out = new ArrayList<>(len);
        for (Chunk c : nonEmptyChunks()) {
            for (int i = 0; i < c.len; i++) out.add(c.point(i));
        }
        return out;
    }

    public Optional<TsPoint<Long>> getAt(long target) {
        for (Chunk c : nonEmptyChunks()) {
            if (target < c.tsMin()) return Optional.empty();
            if (target > c.tsMax()) continue;
            int i = c.firstGe(target);
            if (i < c.len && c.ts[i] == target) return Optional.of(c.point(i));
        }
        return Optional.empty();
    }

    public Optional<TsPoint<Long>> nearestBefore(long target) {
        Chunk best = null;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMin() <= target) best = c;
            else break;
        }
        if (best == null) return Optional.empty();
        int gt = best.firstGt(target);
        return gt == 0 ? Optional.empty() : Optional.of(best.point(gt - 1));
    }

    public List<Long> rangeTimestamps(long lo, long hi) {
        List<Long> out = new ArrayList<>();
        if (lo > hi) return out;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            for (int i = start; i < end; i++) out.add(c.ts[i]);
        }
        return out;
    }

    public Optional<Long> max() {
        boolean any = false;
        long best = Long.MIN_VALUE;
        for (Chunk c : nonEmptyChunks()) {
            for (int i = 0; i < c.len; i++) {
                if (!any || c.val[i] > best) best = c.val[i];
                any = true;
            }
        }
        return any ? Optional.of(best) : Optional.empty();
    }

    public long rangeSum(long lo, long hi) {
        if (lo > hi) return 0L;
        long acc = 0L;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            for (int i = start; i < end; i++) acc += c.val[i];
        }
        return acc;
    }

    public double sum() {
        double acc = 0.0;
        for (Chunk c : nonEmptyChunks()) {
            for (int i = 0; i < c.len; i++) acc += c.val[i];
        }
        return acc;
    }

    @Override
    public double meanOrNaN() {
        return isEmpty() ? Double.NaN : sum() / len;
    }

    public int deleteRange(long lo, long hi) {
        if (lo > hi) return 0;
        int before = len;
        List<Chunk> newWarm = new ArrayList<>();
        Chunk cur = new Chunk(SEAL_CAP);
        int kept = 0;
        List<Chunk> all = new ArrayList<>(warm);
        all.add(head);
        warm.clear();
        head = new Chunk(16);
        for (Chunk c : all) {
            for (int i = 0; i < c.len; i++) {
                if (c.ts[i] < lo || c.ts[i] > hi) {
                    cur.append(c.ts[i], c.val[i]);
                    kept++;
                    if (cur.len == SEAL_CAP) {
                        newWarm.add(cur);
                        cur = new Chunk(SEAL_CAP);
                    }
                }
            }
        }
        warm.addAll(newWarm);
        head = cur;
        len = kept;
        lastTs = last().map(TsPoint::ts).orElse(null);
        return before - kept;
    }
}
