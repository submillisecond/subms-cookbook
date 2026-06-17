package com.submillisecond.recipes.tsdownsampler;

/**
 * Per-bucket rollup. {@code last} is the most recent value seen in the bucket.
 */
public final class TsBucketStats {
    private long count;
    private double sum;
    private double min;
    private double max;
    private double last;

    private TsBucketStats(double value) {
        this.count = 1;
        this.sum = value;
        this.min = value;
        this.max = value;
        this.last = value;
    }

    static TsBucketStats open(double value) {
        return new TsBucketStats(value);
    }

    void update(double value) {
        count++;
        sum += value;
        if (value < min) {
            min = value;
        }
        if (value > max) {
            max = value;
        }
        last = value;
    }

    TsBucketStats copy() {
        TsBucketStats c = new TsBucketStats(0.0);
        c.count = count;
        c.sum = sum;
        c.min = min;
        c.max = max;
        c.last = last;
        return c;
    }

    public long count() {
        return count;
    }

    public double sum() {
        return sum;
    }

    public double min() {
        return min;
    }

    public double max() {
        return max;
    }

    public double last() {
        return last;
    }

    public double mean() {
        if (count == 0) {
            return 0.0;
        }
        return sum / count;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof TsBucketStats s)) return false;
        return count == s.count
                && Double.compare(sum, s.sum) == 0
                && Double.compare(min, s.min) == 0
                && Double.compare(max, s.max) == 0
                && Double.compare(last, s.last) == 0;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(count, sum, min, max, last);
    }

    @Override
    public String toString() {
        return "TsBucketStats{count=" + count + ", sum=" + sum + ", min=" + min
                + ", max=" + max + ", last=" + last + "}";
    }
}
