package com.submillisecond.recipes.tsretention;

import java.util.OptionalInt;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;

/**
 * A retention policy that prunes a {@link TsSeries} (or the primitive
 * {@link TsSeriesD} / {@link TsSeriesL} fast paths) by age, point count, or an
 * approximate byte budget. It carries no storage of its own: every cut is made
 * through the series' own delete surface ({@code truncateBefore} + an
 * index-counter {@code retain}), so it inherits the chunk-rebuild cost model.
 *
 * <p>A policy combines its limits with "most restrictive wins": age is applied
 * first (drop everything older than {@code maxAgeNs} behind the latest point),
 * then the count cap - the tighter of {@code maxPoints} and the point budget
 * implied by {@code maxBytes} - keeps only the newest points.
 *
 * <p>Instances are immutable; the chained setters return a new policy. Byte
 * equivalent to the Rust sibling {@code subms-ts-retention} crate.
 *
 * <pre>
 *   TsSeriesD s = new TsSeriesD();
 *   for (int i = 0; i &lt; 1000; i++) s.push(i, (double) i);
 *   TsRetentionPolicy policy = TsRetentionPolicy.create().maxPoints(100);
 *   int removed = policy.apply(s);   // 900
 *   // s.size() == 100, s.first().ts == 900 (newest 100 kept)
 * </pre>
 */
public final class TsRetentionPolicy {

    /**
     * On-heap footprint charged per point for the byte budget: an {@code i64}
     * timestamp plus an {@code f64}/{@code i64} value-column cell. The SoA
     * storage has no per-point overhead beyond these two columns, so this is
     * the honest per-point cost.
     */
    public static final int BYTES_PER_POINT = 16;

    private final Long maxAgeNs;
    private final Integer maxPoints;
    private final Integer maxBytes;

    private TsRetentionPolicy(Long maxAgeNs, Integer maxPoints, Integer maxBytes) {
        this.maxAgeNs = maxAgeNs;
        this.maxPoints = maxPoints;
        this.maxBytes = maxBytes;
    }

    /** A policy with no limits set. */
    public static TsRetentionPolicy create() {
        return new TsRetentionPolicy(null, null, null);
    }

    /** Keep only points within {@code age} of the most recent timestamp. */
    public TsRetentionPolicy maxAgeNs(long age) {
        return new TsRetentionPolicy(age, this.maxPoints, this.maxBytes);
    }

    /** Keep at most the newest {@code n} points. */
    public TsRetentionPolicy maxPoints(int n) {
        return new TsRetentionPolicy(this.maxAgeNs, n, this.maxBytes);
    }

    /** Keep at most {@code bytes} worth of points (newest first), at {@link #BYTES_PER_POINT} each. */
    public TsRetentionPolicy maxBytes(int bytes) {
        return new TsRetentionPolicy(this.maxAgeNs, this.maxPoints, bytes);
    }

    /**
     * The effective point cap from {@code maxPoints} and {@code maxBytes} (the
     * tighter of the two), or empty if neither is set.
     */
    public OptionalInt pointCap() {
        boolean hasPoints = maxPoints != null;
        boolean hasBytes = maxBytes != null;
        if (hasPoints && hasBytes) {
            return OptionalInt.of(Math.min(maxPoints, maxBytes / BYTES_PER_POINT));
        }
        if (hasPoints) {
            return OptionalInt.of(maxPoints);
        }
        if (hasBytes) {
            return OptionalInt.of(maxBytes / BYTES_PER_POINT);
        }
        return OptionalInt.empty();
    }

    /**
     * Apply the policy to {@code series} (the {@code f64} fast path), returning
     * the number of points removed. Age is applied before the count cap; no-op
     * when the series already fits.
     */
    public int apply(TsSeriesD series) {
        OptionalInt cap = pointCap();

        // Fuse age + count into a single rebuild over the delete surface. The
        // generic path chains truncateBefore + retain, but TsSeriesD's
        // rebuild kernel pre-sizes each fresh chunk to SEAL_CAP regardless of
        // how few points survive, so a separate age pass would zero-fill a
        // 64K-cell chunk to hold a couple thousand kept points. One rebuild
        // that pushes only the survivors keeps the same "keep ts >= cutoff,
        // then newest cap" semantics without that overhead.
        if (maxAgeNs == null && cap.isEmpty()) {
            return 0;
        }

        var points = series.toList();
        int n = points.size();
        if (n == 0) {
            return 0;
        }

        int start = 0;
        if (maxAgeNs != null) {
            long cutoff = saturatingSub(points.get(n - 1).ts(), maxAgeNs);
            while (start < n && points.get(start).ts() < cutoff) {
                start++;
            }
        }
        if (cap.isPresent()) {
            int c = cap.getAsInt();
            int byCount = n - c;
            if (byCount > start) {
                start = byCount;
            }
        }

        if (start == 0) {
            return 0;
        }
        if (start >= n) {
            series.clear();
            return n;
        }
        series.clear();
        for (int i = start; i < n; i++) {
            TsPoint<Double> p = points.get(i);
            series.push(p.ts(), p.value());
        }
        return start;
    }

    /**
     * Apply the policy to a {@code long} fast-path series. {@link TsSeriesL}
     * exposes {@code deleteRange} but not {@code truncateBefore}, so the age cut
     * drops the half-open prefix below the cutoff.
     */
    public int apply(TsSeriesL series) {
        int removed = 0;

        if (maxAgeNs != null) {
            var last = series.last();
            if (last.isPresent()) {
                long cutoff = saturatingSub(last.get().ts(), maxAgeNs);
                if (cutoff > Long.MIN_VALUE) {
                    removed += series.deleteRange(Long.MIN_VALUE, cutoff - 1);
                }
            }
        }

        OptionalInt cap = pointCap();
        if (cap.isPresent()) {
            removed += capTrailingL(series, cap.getAsInt());
        }
        return removed;
    }

    /**
     * Apply the policy to a generic {@code TsSeries<T>}, the faithful mirror of
     * the Rust {@code apply<T>(&mut TsSeries<T>)}: {@code truncateBefore} for
     * age, then an index-counter {@code retain} that keeps the trailing
     * {@code cap} points exactly (tie-safe at duplicate timestamps).
     */
    public <T> int apply(TsSeries<T> series) {
        int removed = 0;

        if (maxAgeNs != null) {
            var last = series.last();
            if (last.isPresent()) {
                removed += series.truncateBefore(saturatingSub(last.get().ts(), maxAgeNs));
            }
        }

        OptionalInt cap = pointCap();
        if (cap.isPresent()) {
            int n = series.size();
            int c = cap.getAsInt();
            if (n > c) {
                int dropBefore = n - c;
                int[] i = {0};
                removed += series.retain(p -> i[0]++ >= dropBefore);
            }
        }
        return removed;
    }

    /**
     * Apply to every series in an iterable (the per-collection case: fold the
     * policy over a collection's series). Returns the total removed.
     */
    public int applyAll(Iterable<TsSeriesD> series) {
        int total = 0;
        for (TsSeriesD s : series) {
            total += apply(s);
        }
        return total;
    }

    // TsSeriesL exposes only deleteRange + rangeTimestamps; drop the leading
    // (n - cap) points by their timestamp window. The series is monotonic, so
    // truncating below the boundary timestamp keeps the trailing cap points
    // (the recipe's i64 streams carry no duplicate timestamps).
    private static int capTrailingL(TsSeriesL series, int cap) {
        int n = series.size();
        if (n <= cap) {
            return 0;
        }
        var last = series.last();
        if (last.isEmpty()) {
            return 0;
        }
        if (cap == 0) {
            return series.deleteRange(Long.MIN_VALUE, last.get().ts());
        }
        int dropBefore = n - cap;
        long boundaryTs = series.rangeTimestamps(Long.MIN_VALUE, Long.MAX_VALUE).get(dropBefore);
        return series.deleteRange(Long.MIN_VALUE, boundaryTs - 1);
    }

    private static long saturatingSub(long a, long b) {
        long r = a - b;
        // overflow iff operands differ in sign and result's sign differs from a.
        if (((a ^ b) & (a ^ r)) < 0) {
            return b > 0 ? Long.MIN_VALUE : Long.MAX_VALUE;
        }
        return r;
    }
}
