package com.submillisecond.recipes.tdigest;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.List;

/**
 * Streaming quantile sketch (t-digest, Ted Dunning): constant memory,
 * mergeable, relative-error bounded - tightest near the tails, which is
 * exactly where p99 / p99.9 live. Add a value stream, ask for any quantile or
 * the CDF at a value, and merge per-shard sketches on a coordinator.
 *
 * <p>This is the "merging" variant with the k1 scale function: centroids near
 * the median absorb more weight, centroids near the tails stay small.
 *
 * <p>The wire format is byte-equivalent to the Rust crate {@code subms-tdigest}
 * in the decode direction: a sketch serialized in Rust deserializes to
 * identical centroids here. Same-input re-encoding is not guaranteed
 * bit-identical across languages (libm asin / merge ordering can diverge by
 * ULPs), but a deserialize then re-serialize of the same bytes round-trips
 * byte-identical.
 */
public final class TsTDigest {

    private static final byte VERSION = 1;
    private static final int HEADER_BYTES = 1 + 28;
    private static final int CENTROID_BYTES = 16;

    private static final class Centroid {
        double mean;
        double weight;

        Centroid(double mean, double weight) {
            this.mean = mean;
            this.weight = weight;
        }
    }

    private final double compression;
    private List<Centroid> centroids;
    private List<Centroid> buffer;
    private double total;
    private double min;
    private double max;

    public TsTDigest(double compression) {
        this.compression = Math.max(compression, 20.0);
        this.centroids = new ArrayList<>();
        this.buffer = new ArrayList<>();
        this.total = 0.0;
        this.min = Double.POSITIVE_INFINITY;
        this.max = Double.NEGATIVE_INFINITY;
    }

    public static TsTDigest withCompression(double compression) {
        return new TsTDigest(compression);
    }

    private TsTDigest(double compression, double min, double max, List<Centroid> centroids) {
        this.compression = compression;
        this.centroids = centroids;
        this.buffer = new ArrayList<>();
        double t = 0.0;
        for (Centroid c : centroids) {
            t += c.weight;
        }
        this.total = t;
        this.min = min;
        this.max = max;
    }

    public double compression() {
        return compression;
    }

    public double count() {
        return total;
    }

    public boolean isEmpty() {
        return total == 0.0;
    }

    public void add(double value) {
        addWeighted(value, 1.0);
    }

    public void addWeighted(double value, double weight) {
        if (!Double.isFinite(value) || weight <= 0.0) {
            return;
        }
        if (value < min) {
            min = value;
        }
        if (value > max) {
            max = value;
        }
        total += weight;
        buffer.add(new Centroid(value, weight));
        if (buffer.size() >= compression * 10.0) {
            process();
        }
    }

    private double k1(double q) {
        double clamped = Math.max(-1.0, Math.min(1.0, 2.0 * q - 1.0));
        return (compression / (2.0 * Math.PI)) * Math.asin(clamped);
    }

    private void process() {
        if (buffer.isEmpty()) {
            return;
        }
        List<Centroid> all = new ArrayList<>(centroids.size() + buffer.size());
        all.addAll(centroids);
        all.addAll(buffer);
        centroids = new ArrayList<>();
        buffer = new ArrayList<>();
        all.sort((a, b) -> Double.compare(a.mean, b.mean));

        double t = total;
        List<Centroid> out = new ArrayList<>(all.size());
        double wFinalized = 0.0;

        for (Centroid c : all) {
            if (out.isEmpty()) {
                out.add(c);
            } else {
                Centroid last = out.get(out.size() - 1);
                double q0 = wFinalized / t;
                double q2 = (wFinalized + last.weight + c.weight) / t;
                if (k1(q2) - k1(q0) <= 1.0) {
                    double w = last.weight + c.weight;
                    last.mean = (last.mean * last.weight + c.mean * c.weight) / w;
                    last.weight = w;
                } else {
                    wFinalized += last.weight;
                    out.add(c);
                }
            }
        }
        centroids = out;
    }

    private void ensureProcessed() {
        if (!buffer.isEmpty()) {
            process();
        }
    }

    /** Fold any buffered points so {@code serialize} sees the final centroids. */
    public void compact() {
        ensureProcessed();
    }

    /** Estimate the value at quantile {@code q} in {@code [0, 1]}. */
    public double quantile(double q) {
        TsTDigest d = folded();
        return d.quantileProcessed(q);
    }

    private double quantileProcessed(double q) {
        List<Centroid> cs = centroids;
        if (cs.isEmpty()) {
            return Double.NaN;
        }
        if (cs.size() == 1) {
            return cs.get(0).mean;
        }
        q = Math.max(0.0, Math.min(1.0, q));
        double target = q * total;

        double[] centers = centers(cs);

        if (target <= centers[0]) {
            double denom = Math.max(centers[0], 1e-300);
            double frac = clamp01(target / denom);
            return min + frac * (cs.get(0).mean - min);
        }
        int last = cs.size() - 1;
        if (target >= centers[last]) {
            double denom = Math.max(total - centers[last], 1e-300);
            double frac = clamp01((target - centers[last]) / denom);
            return cs.get(last).mean + frac * (max - cs.get(last).mean);
        }
        int i = 0;
        while (i + 1 < cs.size() && centers[i + 1] < target) {
            i++;
        }
        double span = Math.max(centers[i + 1] - centers[i], 1e-300);
        double frac = (target - centers[i]) / span;
        return cs.get(i).mean + frac * (cs.get(i + 1).mean - cs.get(i).mean);
    }

    /** Estimate the CDF at {@code value}: the fraction of the distribution &lt;= value. */
    public double cdf(double value) {
        TsTDigest d = folded();
        return d.cdfProcessed(value);
    }

    private double cdfProcessed(double value) {
        List<Centroid> cs = centroids;
        if (cs.isEmpty()) {
            return Double.NaN;
        }
        if (value < min) {
            return 0.0;
        }
        if (value > max) {
            return 1.0;
        }
        double[] centers = centers(cs);
        if (value <= cs.get(0).mean) {
            double denom = Math.max(cs.get(0).mean - min, 1e-300);
            double frac = clamp01((value - min) / denom);
            return (frac * centers[0]) / total;
        }
        int last = cs.size() - 1;
        if (value >= cs.get(last).mean) {
            double denom = Math.max(max - cs.get(last).mean, 1e-300);
            double frac = clamp01((value - cs.get(last).mean) / denom);
            return (centers[last] + frac * (total - centers[last])) / total;
        }
        int i = 0;
        while (i + 1 < cs.size() && cs.get(i + 1).mean < value) {
            i++;
        }
        double span = Math.max(cs.get(i + 1).mean - cs.get(i).mean, 1e-300);
        double frac = (value - cs.get(i).mean) / span;
        return (centers[i] + frac * (centers[i + 1] - centers[i])) / total;
    }

    /** Merge another sketch into a new one (e.g. fold per-shard digests). */
    public TsTDigest merge(TsTDigest other) {
        TsTDigest out = new TsTDigest(Math.max(this.compression, other.compression));
        for (Centroid c : this.centroids) {
            out.addWeighted(c.mean, c.weight);
        }
        for (Centroid c : this.buffer) {
            out.addWeighted(c.mean, c.weight);
        }
        for (Centroid c : other.centroids) {
            out.addWeighted(c.mean, c.weight);
        }
        for (Centroid c : other.buffer) {
            out.addWeighted(c.mean, c.weight);
        }
        out.process();
        return out;
    }

    /**
     * Serialize the folded centroids:
     * {@code [version u8][compression f64][min f64][max f64][count u32]
     * [(mean f64, weight f64) * count]}, all little-endian. Folds any buffered
     * points first on a copy, so {@code this} is unchanged.
     */
    public byte[] serialize() {
        List<Centroid> cs;
        if (buffer.isEmpty()) {
            cs = centroids;
        } else {
            TsTDigest d = copy();
            d.process();
            cs = d.centroids;
        }
        ByteBuffer buf = ByteBuffer.allocate(HEADER_BYTES + cs.size() * CENTROID_BYTES)
                .order(ByteOrder.LITTLE_ENDIAN);
        buf.put(VERSION);
        buf.putDouble(compression);
        buf.putDouble(min);
        buf.putDouble(max);
        buf.putInt(cs.size());
        for (Centroid c : cs) {
            buf.putDouble(c.mean);
            buf.putDouble(c.weight);
        }
        return buf.array();
    }

    public static TsTDigest deserialize(byte[] bytes) {
        if (bytes.length == 0) {
            throw TsTDigestException.truncated();
        }
        if (bytes[0] != VERSION) {
            throw TsTDigestException.badVersion(bytes[0] & 0xff);
        }
        if (bytes.length < HEADER_BYTES) {
            throw TsTDigestException.truncated();
        }
        ByteBuffer buf = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        buf.position(1);
        double compression = buf.getDouble();
        double min = buf.getDouble();
        double max = buf.getDouble();
        long count = buf.getInt() & 0xffffffffL;
        if (bytes.length < HEADER_BYTES + count * CENTROID_BYTES) {
            throw TsTDigestException.truncated();
        }
        List<Centroid> centroids = new ArrayList<>((int) count);
        for (long i = 0; i < count; i++) {
            double mean = buf.getDouble();
            double weight = buf.getDouble();
            centroids.add(new Centroid(mean, weight));
        }
        return new TsTDigest(compression, min, max, centroids);
    }

    private TsTDigest folded() {
        if (buffer.isEmpty()) {
            return this;
        }
        TsTDigest scratch = copy();
        scratch.process();
        return scratch;
    }

    private TsTDigest copy() {
        TsTDigest c = new TsTDigest(compression);
        c.centroids = new ArrayList<>(centroids.size());
        for (Centroid cd : centroids) {
            c.centroids.add(new Centroid(cd.mean, cd.weight));
        }
        c.buffer = new ArrayList<>(buffer.size());
        for (Centroid cd : buffer) {
            c.buffer.add(new Centroid(cd.mean, cd.weight));
        }
        c.total = total;
        c.min = min;
        c.max = max;
        return c;
    }

    private static double[] centers(List<Centroid> cs) {
        double[] centers = new double[cs.size()];
        double cum = 0.0;
        for (int i = 0; i < cs.size(); i++) {
            Centroid c = cs.get(i);
            centers[i] = cum + c.weight / 2.0;
            cum += c.weight;
        }
        return centers;
    }

    private static double clamp01(double v) {
        return Math.max(0.0, Math.min(1.0, v));
    }
}
