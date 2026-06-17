package com.submillisecond.recipes.tsaggregator;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;
import java.util.OptionalDouble;

import com.submillisecond.recipes.ts.TsPoint;

/**
 * Streaming rolling-window aggregator. Push points in time order; read
 * {@code min} / {@code max} / {@code sum} / {@code mean} / {@code count} over
 * the last {@code windowNs} at O(1) amortised per push. Mergeable across
 * partitions for horizontal fan-out. The streaming-query surface of the
 * timeseries arc.
 *
 * <p>min/max use monotonic deques (each point is pushed + popped at most once,
 * so the amortised cost is O(1)); sum/count use a running total + a window
 * buffer.
 *
 * <pre>
 *   TsWindowedAggregator a = new TsWindowedAggregator(1_000); // 1000 ns window
 *   a.push(0, 5.0);
 *   a.push(500, 1.0);
 *   a.push(900, 9.0);
 *   a.min();   // 1.0
 *   a.max();   // 9.0
 *   a.push(1_500, 2.0); // ts 0 + 500 now older than 1000 ns -> expired
 *   a.count(); // 2
 *   a.min();   // 2.0
 * </pre>
 *
 * <p>The window keeps points whose timestamp is within {@code windowNs} of the
 * most recent push ({@code latest - ts < windowNs}).
 */
public final class TsWindowedAggregator {

    private record Pt(long ts, double value) {
    }

    private final long windowNs;
    private final Deque<Pt> buf = new ArrayDeque<>();
    private double sum;
    private final Deque<Pt> minDq = new ArrayDeque<>();
    private final Deque<Pt> maxDq = new ArrayDeque<>();

    public TsWindowedAggregator(long windowNs) {
        this.windowNs = Math.max(windowNs, 1);
    }

    public long windowNs() {
        return windowNs;
    }

    /**
     * Push a point (timestamps must be non-decreasing) and expire anything that
     * has now fallen out of the window.
     */
    public void push(long ts, double value) {
        buf.addLast(new Pt(ts, value));
        sum += value;

        while (!minDq.isEmpty() && minDq.peekLast().value() >= value) {
            minDq.pollLast();
        }
        minDq.addLast(new Pt(ts, value));

        while (!maxDq.isEmpty() && maxDq.peekLast().value() <= value) {
            maxDq.pollLast();
        }
        maxDq.addLast(new Pt(ts, value));

        expire(ts);
    }

    private void expire(long now) {
        long cutoff = now - windowNs;
        while (!buf.isEmpty() && buf.peekFirst().ts() <= cutoff) {
            sum -= buf.pollFirst().value();
        }
        while (!minDq.isEmpty() && minDq.peekFirst().ts() <= cutoff) {
            minDq.pollFirst();
        }
        while (!maxDq.isEmpty() && maxDq.peekFirst().ts() <= cutoff) {
            maxDq.pollFirst();
        }
    }

    public int count() {
        return buf.size();
    }

    public boolean isEmpty() {
        return buf.isEmpty();
    }

    public double sum() {
        return sum;
    }

    public OptionalDouble min() {
        return minDq.isEmpty() ? OptionalDouble.empty() : OptionalDouble.of(minDq.peekFirst().value());
    }

    public OptionalDouble max() {
        return maxDq.isEmpty() ? OptionalDouble.empty() : OptionalDouble.of(maxDq.peekFirst().value());
    }

    public OptionalDouble mean() {
        return buf.isEmpty() ? OptionalDouble.empty() : OptionalDouble.of(sum / buf.size());
    }

    /** The points currently in the window, in time order. */
    public List<TsPoint<Double>> window() {
        List<TsPoint<Double>> out = new ArrayList<>(buf.size());
        for (Pt p : buf) {
            out.add(new TsPoint<>(p.ts(), p.value()));
        }
        return out;
    }

    /**
     * Merge another aggregator (same logical window) into a new one, e.g. to
     * fold per-partition windows on a coordinator. Points from both are
     * replayed in time order, so the result is the correct rolling state as of
     * the latest timestamp across both.
     */
    public TsWindowedAggregator merge(TsWindowedAggregator other) {
        List<Pt> all = new ArrayList<>(buf.size() + other.buf.size());
        all.addAll(buf);
        all.addAll(other.buf);
        all.sort((a, b) -> Long.compare(a.ts(), b.ts()));
        TsWindowedAggregator out = new TsWindowedAggregator(Math.max(windowNs, other.windowNs));
        for (Pt p : all) {
            out.push(p.ts(), p.value());
        }
        return out;
    }

    // ---------- distributed-merge wire format ----------

    private static final byte WIRE_VERSION = 1;
    private static final int WIRE_HEADER = 1 + 8 + 4; // version + windowNs + count
    private static final int WIRE_POINT = 8 + 8;      // ts + value bits

    /**
     * Serialise the partial window for shipping to a coordinator that will
     * {@link #merge} it. Only {@code windowNs} + the in-window points cross the
     * wire; the sum and min/max deques are derived state that {@link #fromWire}
     * rebuilds by replaying the points. Little-endian wire form:
     *
     * <pre>[version u8][windowNs i64][count u32][count x (ts i64, valueBits u64)]</pre>
     *
     * Byte-equivalent to the Rust {@code subms-ts-aggregator} crate's
     * {@code to_wire}: a partial window encoded on a Java shard decodes on a
     * Rust coordinator and vice versa.
     */
    public byte[] toWire() {
        ByteBuffer bb = ByteBuffer.allocate(WIRE_HEADER + buf.size() * WIRE_POINT)
                .order(ByteOrder.LITTLE_ENDIAN);
        bb.put(WIRE_VERSION);
        bb.putLong(windowNs);
        bb.putInt(buf.size());
        for (Pt p : buf) {
            bb.putLong(p.ts());
            bb.putLong(Double.doubleToLongBits(p.value()));
        }
        return bb.array();
    }

    /** Reconstruct a partial window from {@link #toWire} bytes. */
    public static TsWindowedAggregator fromWire(byte[] bytes) {
        if (bytes.length == 0) {
            throw TsAggWireException.truncated();
        }
        if (bytes[0] != WIRE_VERSION) {
            throw TsAggWireException.badVersion(bytes[0] & 0xff);
        }
        if (bytes.length < WIRE_HEADER) {
            throw TsAggWireException.truncated();
        }
        ByteBuffer bb = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        bb.get(); // version, already checked
        long windowNs = bb.getLong();
        int count = bb.getInt();
        long need = (long) WIRE_HEADER + Integer.toUnsignedLong(count) * WIRE_POINT;
        if (bytes.length < need) {
            throw TsAggWireException.truncated();
        }
        TsWindowedAggregator agg = new TsWindowedAggregator(windowNs);
        for (int i = 0; i < count; i++) {
            long ts = bb.getLong();
            double value = Double.longBitsToDouble(bb.getLong());
            agg.push(ts, value);
        }
        return agg;
    }
}
