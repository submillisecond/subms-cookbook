package com.submillisecond.recipes.tsaggregator;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.OptionalDouble;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsPoint;

class TsWindowedAggregatorTest {

    @Test
    void emptyAggregator() {
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        assertTrue(a.isEmpty());
        assertEquals(0, a.count());
        assertTrue(a.min().isEmpty());
        assertTrue(a.max().isEmpty());
        assertTrue(a.mean().isEmpty());
        assertEquals(0.0, a.sum());
    }

    @Test
    void basicAggregates() {
        TsWindowedAggregator a = new TsWindowedAggregator(10_000);
        a.push(0, 5.0);
        a.push(1, 1.0);
        a.push(2, 9.0);
        a.push(3, 3.0);
        assertEquals(4, a.count());
        assertFalse(a.isEmpty());
        assertEquals(OptionalDouble.of(1.0), a.min());
        assertEquals(OptionalDouble.of(9.0), a.max());
        assertEquals(18.0, a.sum());
        assertEquals(OptionalDouble.of(4.5), a.mean());
    }

    @Test
    void windowExpiry() {
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        a.push(0, 5.0);
        a.push(500, 1.0);
        a.push(900, 9.0);
        assertEquals(3, a.count());
        a.push(1_500, 2.0);
        assertEquals(2, a.count());
        assertEquals(OptionalDouble.of(2.0), a.min());
        assertEquals(OptionalDouble.of(9.0), a.max());
        assertEquals(11.0, a.sum());
    }

    @Test
    void minMaxRecoverAfterExpiry() {
        TsWindowedAggregator a = new TsWindowedAggregator(100);
        a.push(0, 1.0);
        a.push(50, 5.0);
        a.push(90, 3.0);
        assertEquals(OptionalDouble.of(1.0), a.min());
        a.push(101, 4.0);
        assertEquals(OptionalDouble.of(3.0), a.min());
        assertEquals(OptionalDouble.of(5.0), a.max());
    }

    @Test
    void maxRecoversAfterExpiry() {
        TsWindowedAggregator a = new TsWindowedAggregator(100);
        a.push(0, 9.0);
        a.push(50, 2.0);
        a.push(90, 4.0);
        assertEquals(OptionalDouble.of(9.0), a.max());
        a.push(101, 1.0);
        assertEquals(OptionalDouble.of(4.0), a.max());
        assertEquals(OptionalDouble.of(1.0), a.min());
    }

    @Test
    void duplicateValuesHandled() {
        TsWindowedAggregator a = new TsWindowedAggregator(10_000);
        for (long t = 0; t < 10; t++) {
            a.push(t, 5.0);
        }
        assertEquals(OptionalDouble.of(5.0), a.min());
        assertEquals(OptionalDouble.of(5.0), a.max());
        assertEquals(10, a.count());
        assertEquals(50.0, a.sum());
    }

    @Test
    void windowIterInOrder() {
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        a.push(10, 1.0);
        a.push(20, 2.0);
        a.push(30, 3.0);
        List<TsPoint<Double>> pts = a.window();
        assertEquals(List.of(
                new TsPoint<>(10L, 1.0),
                new TsPoint<>(20L, 2.0),
                new TsPoint<>(30L, 3.0)), pts);
    }

    @Test
    void windowNsClampedToOne() {
        TsWindowedAggregator a = new TsWindowedAggregator(0);
        assertEquals(1, a.windowNs());
        a.push(0, 1.0);
        a.push(1, 2.0);
        assertEquals(1, a.count());
        assertEquals(OptionalDouble.of(2.0), a.min());
    }

    @Test
    void mergePartitions() {
        TsWindowedAggregator a = new TsWindowedAggregator(10_000);
        a.push(0, 1.0);
        a.push(100, 3.0);
        TsWindowedAggregator b = new TsWindowedAggregator(10_000);
        b.push(50, 9.0);
        b.push(150, 2.0);
        TsWindowedAggregator m = a.merge(b);
        assertEquals(4, m.count());
        assertEquals(OptionalDouble.of(1.0), m.min());
        assertEquals(OptionalDouble.of(9.0), m.max());
        assertEquals(15.0, m.sum());
        List<Long> ts = new ArrayList<>();
        for (TsPoint<Double> p : m.window()) {
            ts.add(p.ts());
        }
        assertEquals(List.of(0L, 50L, 100L, 150L), ts);
    }

    @Test
    void mergeAppliesWindowExpiry() {
        TsWindowedAggregator a = new TsWindowedAggregator(100);
        a.push(0, 1.0);
        TsWindowedAggregator b = new TsWindowedAggregator(100);
        b.push(200, 2.0);
        TsWindowedAggregator m = a.merge(b);
        assertEquals(1, m.count());
        assertEquals(OptionalDouble.of(2.0), m.min());
    }

    @Test
    void slidingCorrectnessVsNaive() {
        long window = 1_000L;
        TsWindowedAggregator a = new TsWindowedAggregator(window);
        List<long[]> tsList = new ArrayList<>();
        List<Double> valList = new ArrayList<>();
        long state = 12345L;
        for (long i = 0; i < 5_000L; i++) {
            state ^= state << 13;
            state ^= state >>> 7;
            state ^= state << 17;
            double v = (state >>> 40) / 1_000.0;
            long ts = i * 10;
            a.push(ts, v);
            tsList.add(new long[] { ts });
            valList.add(v);

            long cutoff = ts - window;
            double bfMin = Double.POSITIVE_INFINITY;
            double bfMax = Double.NEGATIVE_INFINITY;
            int live = 0;
            for (int j = 0; j < tsList.size(); j++) {
                if (tsList.get(j)[0] > cutoff) {
                    double vv = valList.get(j);
                    bfMin = Math.min(bfMin, vv);
                    bfMax = Math.max(bfMax, vv);
                    live++;
                }
            }
            assertEquals(bfMin, a.min().getAsDouble(), "min mismatch at i=" + i);
            assertEquals(bfMax, a.max().getAsDouble(), "max mismatch at i=" + i);
            assertEquals(live, a.count());
        }
    }

    // ---------- distributed-merge wire format ----------

    // Same canonical fixture the Rust crate pins: window 1000, points
    // (100,1) (200,2) (300,3). Identical hex proves the cross-language layout.
    private static final String WIRE_FIXTURE =
        "01e803000000000000030000006400000000000000000000000000f03fc80000000000000000000000000000402c010000000000000000000000000840";

    private static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            sb.append(String.format("%02x", b & 0xff));
        }
        return sb.toString();
    }

    @Test
    void wireMatchesFixture() {
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        a.push(100, 1.0);
        a.push(200, 2.0);
        a.push(300, 3.0);
        assertEquals(WIRE_FIXTURE, toHex(a.toWire()));
    }

    @Test
    void wireRoundTrips() {
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        for (int i = 0; i < 500; i++) {
            a.push(i * 3L, Math.sin(i * 0.5));
        }
        TsWindowedAggregator back = TsWindowedAggregator.fromWire(a.toWire());
        assertEquals(a.windowNs(), back.windowNs());
        assertEquals(a.count(), back.count());
        assertEquals(a.min(), back.min());
        assertEquals(a.max(), back.max());
        // sum is a running total whose expiry history differs from the replay,
        // so it agrees only to FP tolerance.
        assertTrue(Math.abs(back.sum() - a.sum()) <= Math.abs(a.sum()) * 1e-12 + 1e-9);
    }

    @Test
    void mergeAcrossTheWire() {
        TsWindowedAggregator s1 = new TsWindowedAggregator(1_000);
        TsWindowedAggregator s2 = new TsWindowedAggregator(1_000);
        for (int i = 0; i < 200; i++) {
            s1.push(i * 2L, i);
            s2.push(i * 2L + 1, i * 2.0);
        }
        TsWindowedAggregator d1 = TsWindowedAggregator.fromWire(s1.toWire());
        TsWindowedAggregator d2 = TsWindowedAggregator.fromWire(s2.toWire());
        TsWindowedAggregator coordinator = d1.merge(d2);
        TsWindowedAggregator direct = s1.merge(s2);
        assertEquals(direct.count(), coordinator.count());
        assertEquals(direct.sum(), coordinator.sum());
        assertEquals(direct.min(), coordinator.min());
        assertEquals(direct.max(), coordinator.max());
    }

    @Test
    void wireEmptyWindow() {
        TsWindowedAggregator a = new TsWindowedAggregator(500);
        TsWindowedAggregator back = TsWindowedAggregator.fromWire(a.toWire());
        assertEquals(0, back.count());
        assertEquals(500, back.windowNs());
    }

    @Test
    void wireRejectsBadVersion() {
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        a.push(1, 1.0);
        byte[] bytes = a.toWire();
        bytes[0] = 99;
        TsAggWireException ex = assertThrows(TsAggWireException.class, () -> TsWindowedAggregator.fromWire(bytes));
        assertEquals(TsAggWireException.Kind.BAD_VERSION, ex.kind());
        assertEquals(99, ex.version());
    }

    @Test
    void wireRejectsTruncated() {
        assertThrows(TsAggWireException.class, () -> TsWindowedAggregator.fromWire(new byte[0]));
        TsWindowedAggregator a = new TsWindowedAggregator(1_000);
        a.push(1, 1.0);
        a.push(2, 2.0);
        byte[] full = a.toWire();
        byte[] cut = new byte[full.length - 4];
        System.arraycopy(full, 0, cut, 0, cut.length);
        TsAggWireException ex = assertThrows(TsAggWireException.class, () -> TsWindowedAggregator.fromWire(cut));
        assertEquals(TsAggWireException.Kind.TRUNCATED, ex.kind());
    }
}
