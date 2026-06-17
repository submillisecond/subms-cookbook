package com.submillisecond.recipes.hdrhist.features;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Histogram with per-recording 1-byte tags for downstream slicing.
 *
 * <p>Storage: each base bucket carries a small per-tag map of counts.
 * The total count is the sum across all tags. Per-tag percentile
 * reads iterate over buckets and accumulate only the matching tag's
 * count.
 *
 * <p>Trade-off vs N separate histograms:
 * <ul>
 *   <li>Cheaper when tag cardinality is small but value range is wide
 *       (one bucket array shared across tags).</li>
 *   <li>More expensive per-tag percentile read (walks all buckets).</li>
 *   <li>Constant overhead per write is one extra map lookup.</li>
 * </ul>
 */
public final class TaggedHdrHistogram {

    private final int subCountBits;
    /** Outer index = bucket; inner map = tag -> count. */
    @SuppressWarnings("unchecked")
    private Map<Byte, Long>[] buckets;
    private long total;
    private int highIndex;
    private final Map<Byte, Long> perTagTotal = new HashMap<>();

    public TaggedHdrHistogram(int significantDigits) {
        int sig = Math.min(5, Math.max(1, significantDigits));
        int target = 2 * (int) Math.pow(10, sig);
        int bits = Math.max(1, 32 - Integer.numberOfLeadingZeros(target));
        this.subCountBits = bits;
        int subCount = 1 << bits;
        @SuppressWarnings("unchecked")
        Map<Byte, Long>[] arr = new Map[subCount];
        for (int i = 0; i < subCount; i++) arr[i] = new HashMap<>();
        this.buckets = arr;
    }

    /** Record a value tagged with the given byte. */
    public void record(long value, byte tag) {
        int idx = indexOf(value);
        if (idx >= buckets.length) {
            @SuppressWarnings("unchecked")
            Map<Byte, Long>[] grown = new Map[idx + 1];
            System.arraycopy(buckets, 0, grown, 0, buckets.length);
            for (int i = buckets.length; i < grown.length; i++) grown[i] = new HashMap<>();
            buckets = grown;
        }
        buckets[idx].merge(tag, 1L, Long::sum);
        total++;
        if (idx > highIndex) highIndex = idx;
        perTagTotal.merge(tag, 1L, Long::sum);
    }

    public long count() { return total; }

    public long countForTag(byte tag) {
        return perTagTotal.getOrDefault(tag, 0L);
    }

    public long max() {
        if (total == 0) return 0;
        return valueFromIndex(highIndex);
    }

    /** Quantile across all tags (matches the base histogram). */
    public long valueAtPercentile(double q) {
        if (total == 0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        long target = Math.max(1L, (long) (qc * total));
        long cum = 0;
        int end = Math.min(highIndex + 1, buckets.length);
        for (int i = 0; i < end; i++) {
            for (long c : buckets[i].values()) cum += c;
            if (cum >= target) return valueFromIndex(i);
        }
        return valueFromIndex(highIndex);
    }

    /** Quantile restricted to one tag. */
    public long valueAtPercentileForTag(double q, byte tag) {
        long tagTotal = countForTag(tag);
        if (tagTotal == 0) return 0;
        double qc = Math.min(1.0, Math.max(0.0, q));
        long target = Math.max(1L, (long) (qc * tagTotal));
        long cum = 0;
        int end = Math.min(highIndex + 1, buckets.length);
        for (int i = 0; i < end; i++) {
            Long c = buckets[i].get(tag);
            if (c == null) continue;
            cum += c;
            if (cum >= target) return valueFromIndex(i);
        }
        return valueFromIndex(highIndex);
    }

    /** Tags seen in any recording, in ascending byte order. */
    public List<Byte> tags() {
        List<Byte> out = new ArrayList<>(perTagTotal.keySet());
        Collections.sort(out);
        return out;
    }

    private int indexOf(long value) {
        long subMask = (1L << subCountBits) - 1;
        if (value <= subMask) return (int) value;
        int bits = 64 - Long.numberOfLeadingZeros(value);
        int major = bits - subCountBits;
        long sub = (value >>> (major - 1)) & subMask;
        return (major << subCountBits) | (int) sub;
    }

    private long valueFromIndex(int idx) {
        long subCnt = 1L << subCountBits;
        long subMask = subCnt - 1;
        long i = idx;
        if (i < subCnt) return i;
        long major = i >>> subCountBits;
        long sub = i & subMask;
        return (sub | subCnt) << (major - 1);
    }

}
