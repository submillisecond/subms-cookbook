package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

/**
 * Unboxed scalar-{@code double} time series. Primitive {@code long[]} ts +
 * {@code double[]} value columns per chunk - no boxing on push or scan. This
 * is the fast path the sub-ms claim runs on; it is an independent class (no
 * inheritance from {@link TsSeries}) so the storage stays primitive.
 */
public final class TsSeriesD implements TsNumericSeries {

    static final int SEAL_CAP = 65_536;

    private static final class Chunk {
        long[] ts;
        double[] val;
        int len;

        Chunk(int cap) {
            int c = Math.max(1, cap);
            this.ts = new long[c];
            this.val = new double[c];
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

        void append(long t, double v) {
            if (len == ts.length) {
                long[] gt = new long[ts.length * 2];
                double[] gv = new double[ts.length * 2];
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

        TsPoint<Double> point(int i) {
            return new TsPoint<>(ts[i], val[i]);
        }

        TsPoint<Double> removeAt(int i) {
            long t = ts[i];
            double v = val[i];
            System.arraycopy(ts, i + 1, ts, i, len - i - 1);
            System.arraycopy(val, i + 1, val, i, len - i - 1);
            len--;
            return new TsPoint<>(t, v);
        }
    }

    private final List<Chunk> warm = new ArrayList<>();
    private Chunk head = new Chunk(16);
    private int len = 0;
    private Long lastTs = null;
    private TsSeriesMetadata meta = null;

    public TsSeriesD() {}

    public static TsSeriesD withCapacity(int cap) {
        TsSeriesD s = new TsSeriesD();
        s.head = new Chunk(Math.min(Math.max(1, cap), SEAL_CAP));
        return s;
    }

    public static TsSeriesD fromPoints(List<TsPoint<Double>> points) {
        TsSeriesD s = withCapacity(points.size());
        for (TsPoint<Double> p : points) {
            s.push(p.ts(), p.value());
        }
        return s;
    }

    public TsSeriesD withMetadata(TsSeriesMetadata meta) {
        this.meta = meta;
        return this;
    }

    public Optional<TsSeriesMetadata> metadata() {
        return Optional.ofNullable(meta);
    }

    public void setMetadata(TsSeriesMetadata meta) {
        this.meta = meta;
    }

    public void push(long ts, double value) {
        if (!Double.isFinite(value)) {
            throw TsException.nullValue("non-finite or null observation");
        }
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

    public Optional<TsPoint<Double>> first() {
        for (Chunk c : warm) {
            if (!c.isEmpty()) return Optional.of(c.point(0));
        }
        return head.isEmpty() ? Optional.empty() : Optional.of(head.point(0));
    }

    public Optional<TsPoint<Double>> last() {
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

    public List<TsPoint<Double>> toList() {
        List<TsPoint<Double>> out = new ArrayList<>(len);
        for (Chunk c : nonEmptyChunks()) {
            for (int i = 0; i < c.len; i++) out.add(c.point(i));
        }
        return out;
    }

    public Optional<TsPoint<Double>> getAt(long target) {
        for (Chunk c : nonEmptyChunks()) {
            if (target < c.tsMin()) return Optional.empty();
            if (target > c.tsMax()) continue;
            int i = c.firstGe(target);
            if (i < c.len && c.ts[i] == target) return Optional.of(c.point(i));
        }
        return Optional.empty();
    }

    public Optional<TsPoint<Double>> nearestBefore(long target) {
        Chunk best = null;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMin() <= target) best = c;
            else break;
        }
        if (best == null) return Optional.empty();
        int gt = best.firstGt(target);
        return gt == 0 ? Optional.empty() : Optional.of(best.point(gt - 1));
    }

    public Optional<TsPoint<Double>> nearestAfter(long target) {
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < target) continue;
            int i = c.firstGe(target);
            if (i < c.len) return Optional.of(c.point(i));
        }
        return Optional.empty();
    }

    public Optional<TsPoint<Double>> nearest(long target) {
        Optional<TsPoint<Double>> before = nearestBefore(target);
        Optional<TsPoint<Double>> after = nearestAfter(target);
        if (before.isPresent() && after.isPresent()) {
            TsPoint<Double> b = before.get();
            TsPoint<Double> a = after.get();
            return (target - b.ts()) <= (a.ts() - target) ? before : after;
        }
        return before.isPresent() ? before : after;
    }

    // ---------- vectorisation-friendly reduction kernels ----------
    //
    // HotSpot's superword (auto-vectorisation) pass handles these at runtime
    // - the JVM analogue of the Rust `simd` feature, with no incubator
    // module. The shapes are chosen by measurement, not symmetry: a single
    // f64 accumulator will NOT auto-vectorise (FP add is non-associative), so
    // `sum` is unrolled into eight independent lane accumulators that C2 then
    // packs into vector adds (measured ~6x); `min` / `max` are left as a
    // plain counted loop because C2 already vectorises that form and a manual
    // unroll only adds register pressure (measured slower). The lane-summed
    // f64 can differ from a strict left-fold by an ULP; min/max stay exact
    // because NaN is rejected on ingest. Callers pass the valid region
    // [from, to) and guarantee from < to for min/max.

    private static final int LANES = 8;

    private static double sumSlice(double[] v, int from, int to) {
        double a0 = 0, a1 = 0, a2 = 0, a3 = 0, a4 = 0, a5 = 0, a6 = 0, a7 = 0;
        int i = from;
        for (; i + LANES <= to; i += LANES) {
            a0 += v[i];     a1 += v[i + 1]; a2 += v[i + 2]; a3 += v[i + 3];
            a4 += v[i + 4]; a5 += v[i + 5]; a6 += v[i + 6]; a7 += v[i + 7];
        }
        double acc = ((a0 + a1) + (a2 + a3)) + ((a4 + a5) + (a6 + a7));
        for (; i < to; i++) acc += v[i];
        return acc;
    }

    private static double minSlice(double[] v, int from, int to) {
        double m = v[from];
        for (int i = from + 1; i < to; i++) if (v[i] < m) m = v[i];
        return m;
    }

    private static double maxSlice(double[] v, int from, int to) {
        double m = v[from];
        for (int i = from + 1; i < to; i++) if (v[i] > m) m = v[i];
        return m;
    }

    // ---------- ranged aggregates over the primitive columns ----------

    public Optional<Double> rangeMin(long lo, long hi) {
        if (lo > hi) return Optional.empty();
        boolean any = false;
        double best = Double.POSITIVE_INFINITY;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            if (start < end) {
                double m = minSlice(c.val, start, end);
                if (!any || m < best) best = m;
                any = true;
            }
        }
        return any ? Optional.of(best) : Optional.empty();
    }

    public Optional<Double> rangeMax(long lo, long hi) {
        if (lo > hi) return Optional.empty();
        boolean any = false;
        double best = Double.NEGATIVE_INFINITY;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            if (start < end) {
                double m = maxSlice(c.val, start, end);
                if (!any || m > best) best = m;
                any = true;
            }
        }
        return any ? Optional.of(best) : Optional.empty();
    }

    public double rangeSum(long lo, long hi) {
        if (lo > hi) return 0.0;
        double acc = 0.0;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            if (start < end) acc += sumSlice(c.val, start, end);
        }
        return acc;
    }

    public Optional<Double> rangeMean(long lo, long hi) {
        if (lo > hi) return Optional.empty();
        double acc = 0.0;
        int count = 0;
        for (Chunk c : nonEmptyChunks()) {
            if (c.tsMax() < lo || c.tsMin() > hi) continue;
            int start = c.firstGe(lo);
            int end = c.firstGt(hi);
            if (start < end) {
                acc += sumSlice(c.val, start, end);
                count += end - start;
            }
        }
        return count == 0 ? Optional.empty() : Optional.of(acc / count);
    }

    public Optional<Double> min() {
        boolean any = false;
        double best = Double.POSITIVE_INFINITY;
        for (Chunk c : nonEmptyChunks()) {
            double m = minSlice(c.val, 0, c.len);
            if (!any || m < best) best = m;
            any = true;
        }
        return any ? Optional.of(best) : Optional.empty();
    }

    public Optional<Double> max() {
        boolean any = false;
        double best = Double.NEGATIVE_INFINITY;
        for (Chunk c : nonEmptyChunks()) {
            double m = maxSlice(c.val, 0, c.len);
            if (!any || m > best) best = m;
            any = true;
        }
        return any ? Optional.of(best) : Optional.empty();
    }

    public double sum() {
        double acc = 0.0;
        for (Chunk c : nonEmptyChunks()) {
            acc += sumSlice(c.val, 0, c.len);
        }
        return acc;
    }

    public Optional<Double> mean() {
        return isEmpty() ? Optional.empty() : Optional.of(sum() / len);
    }

    @Override
    public double meanOrNaN() {
        return mean().orElse(Double.NaN);
    }

    // ---------- delete surface ----------

    public Optional<TsPoint<Double>> deleteAt(long target) {
        for (int ci = 0; ci <= warm.size(); ci++) {
            Chunk c = ci < warm.size() ? warm.get(ci) : head;
            if (c.isEmpty() || target < c.tsMin() || target > c.tsMax()) continue;
            int i = c.firstGe(target);
            if (i < c.len && c.ts[i] == target) {
                TsPoint<Double> removed = c.removeAt(i);
                len--;
                warm.removeIf(Chunk::isEmpty);
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

    public int truncateBefore(long cutoff) {
        return retainPoints((ts, v) -> ts >= cutoff);
    }

    public int truncateAfter(long cutoff) {
        return retainPoints((ts, v) -> ts <= cutoff);
    }

    public void clear() {
        warm.clear();
        head = new Chunk(16);
        len = 0;
        lastTs = null;
    }

    private interface KeepFn {
        boolean keep(long ts, double v);
    }

    private int retainPoints(KeepFn keep) {
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
                if (keep.keep(c.ts[i], c.val[i])) {
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
        recomputeLastTs();
        return before - kept;
    }

    private void recomputeLastTs() {
        lastTs = last().map(TsPoint::ts).orElse(null);
    }
}
