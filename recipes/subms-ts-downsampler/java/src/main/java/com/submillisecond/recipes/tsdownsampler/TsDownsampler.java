package com.submillisecond.recipes.tsdownsampler;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsSeries;

/**
 * A tiered downsampling pipeline. Push raw points once; each tier (e.g. 1s,
 * 1m, 1h) rolls them into fixed-width buckets, emitting
 * {@code (count, sum, min, max, last)} per bucket. A query then answers at
 * whatever resolution a tier provides without rescanning raw data - the
 * write-path side of multi-resolution storage.
 *
 * <p>Tier 0 is the finest. One {@code push} touches every tier, each O(1)
 * amortised.
 */
public final class TsDownsampler {

    private static final class Tier {
        final long durationNs;
        final TsSeries<Double> means = new TsSeries<>();
        final List<Long> closedStarts = new ArrayList<>();
        final List<TsBucketStats> closedStats = new ArrayList<>();
        boolean hasOpen = false;
        long openStart = 0;
        TsBucketStats open = TsBucketStats.open(0.0);

        Tier(long durationNs) {
            this.durationNs = Math.max(1, durationNs);
        }

        long bucketStart(long ts) {
            return Math.floorDiv(ts, durationNs) * durationNs;
        }

        void push(long ts, double value) {
            long start = bucketStart(ts);
            if (hasOpen && openStart == start) {
                open.update(value);
            } else if (hasOpen) {
                close(openStart);
                openStart = start;
                open = TsBucketStats.open(value);
            } else {
                hasOpen = true;
                openStart = start;
                open = TsBucketStats.open(value);
            }
        }

        void close(long start) {
            means.push(start, open.mean());
            closedStarts.add(start);
            closedStats.add(open.copy());
        }

        void flush() {
            if (hasOpen) {
                close(openStart);
                hasOpen = false;
            }
        }

        Optional<TsBucketStats> statsAt(long ts) {
            long start = bucketStart(ts);
            if (hasOpen && openStart == start) {
                return Optional.of(open.copy());
            }
            int i = binarySearch(start);
            if (i >= 0) {
                return Optional.of(closedStats.get(i).copy());
            }
            return Optional.empty();
        }

        private int binarySearch(long start) {
            int lo = 0;
            int hi = closedStarts.size() - 1;
            while (lo <= hi) {
                int mid = (lo + hi) >>> 1;
                long s = closedStarts.get(mid);
                if (s < start) {
                    lo = mid + 1;
                } else if (s > start) {
                    hi = mid - 1;
                } else {
                    return mid;
                }
            }
            return -1;
        }
    }

    private final List<Tier> tiers;

    /** One tier per bucket duration (nanoseconds), finest first. */
    public TsDownsampler(long[] tierDurationsNs) {
        this.tiers = new ArrayList<>(tierDurationsNs.length);
        for (long d : tierDurationsNs) {
            tiers.add(new Tier(d));
        }
    }

    public int tierCount() {
        return tiers.size();
    }

    /** Feed a raw point to every tier. */
    public void push(long ts, double value) {
        for (Tier t : tiers) {
            t.push(ts, value);
        }
    }

    /** Close every tier's open bucket so the final partial bucket is emitted. */
    public void flush() {
        for (Tier t : tiers) {
            t.flush();
        }
    }

    /** The closed-bucket mean series for a tier (bucketStart -> mean). */
    public TsSeries<Double> tier(int level) {
        return tiers.get(level).means;
    }

    public long tierDuration(int level) {
        return tiers.get(level).durationNs;
    }

    /**
     * Full bucket stats for the bucket containing {@code ts} at {@code level}
     * (open or closed). Empty if no point has landed in that bucket.
     */
    public Optional<TsBucketStats> bucketStats(int level, long ts) {
        if (level < 0 || level >= tiers.size()) {
            return Optional.empty();
        }
        return tiers.get(level).statsAt(ts);
    }
}
