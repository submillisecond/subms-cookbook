package com.submillisecond.recipes.tdigest;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;

import org.junit.jupiter.api.Test;

final class TsTDigestTest {

    private static double exactQuantile(double[] sorted, double q) {
        if (sorted.length == 0) {
            return Double.NaN;
        }
        int idx = (int) Math.round(q * (sorted.length - 1));
        return sorted[Math.min(idx, sorted.length - 1)];
    }

    private static byte[] fromHex(String hex) {
        byte[] out = new byte[hex.length() / 2];
        for (int i = 0; i < out.length; i++) {
            int hi = Character.digit(hex.charAt(i * 2), 16);
            int lo = Character.digit(hex.charAt(i * 2 + 1), 16);
            out[i] = (byte) ((hi << 4) | lo);
        }
        return out;
    }

    @Test
    void emptyDigest() {
        TsTDigest d = new TsTDigest(100.0);
        assertTrue(d.isEmpty());
        assertTrue(Double.isNaN(d.quantile(0.5)));
        assertEquals(0.0, d.count());
    }

    @Test
    void singleValue() {
        TsTDigest d = new TsTDigest(100.0);
        d.add(42.0);
        assertEquals(42.0, d.quantile(0.0));
        assertEquals(42.0, d.quantile(0.5));
        assertEquals(42.0, d.quantile(1.0));
    }

    @Test
    void minMaxExactAtEdges() {
        TsTDigest d = new TsTDigest(100.0);
        for (int i = 0; i < 10_000; i++) {
            d.add(i);
        }
        assertEquals(0.0, d.quantile(0.0));
        assertEquals(9_999.0, d.quantile(1.0));
    }

    @Test
    void uniformQuantilesWithinBound() {
        int n = 100_000;
        double[] vals = new double[n];
        long state = 0x2545F4914F6CDD1DL;
        for (int i = 0; i < n; i++) {
            state ^= state << 13;
            state ^= state >>> 7;
            state ^= state << 17;
            vals[i] = (state >>> 11) / (double) (1L << 53);
        }
        TsTDigest d = new TsTDigest(200.0);
        for (double v : vals) {
            d.add(v);
        }
        double[] sorted = vals.clone();
        Arrays.sort(sorted);

        for (double q : new double[] {0.01, 0.1, 0.5, 0.9, 0.99, 0.999}) {
            double est = d.quantile(q);
            double exact = exactQuantile(sorted, q);
            assertTrue(Math.abs(est - exact) < 0.02,
                    "q=" + q + ": est=" + est + ", exact=" + exact + ", err=" + Math.abs(est - exact));
        }
    }

    @Test
    void tailIsTighterThanMedian() {
        int n = 100_000;
        double[] vals = new double[n];
        for (int i = 0; i < n; i++) {
            vals[i] = i / (double) n;
        }
        TsTDigest d = new TsTDigest(200.0);
        for (double v : vals) {
            d.add(v);
        }
        Arrays.sort(vals);
        double err50 = Math.abs(d.quantile(0.5) - exactQuantile(vals, 0.5));
        double err999 = Math.abs(d.quantile(0.999) - exactQuantile(vals, 0.999));
        assertTrue(err999 <= err50 + 1e-9,
                "tail err " + err999 + " should be <= median err " + err50);
    }

    @Test
    void cdfRoundtripsWithQuantile() {
        TsTDigest d = new TsTDigest(200.0);
        for (int i = 0; i < 10_000; i++) {
            d.add(i);
        }
        for (double q : new double[] {0.1, 0.5, 0.9}) {
            double v = d.quantile(q);
            double back = d.cdf(v);
            assertTrue(Math.abs(back - q) < 0.02, "q=" + q + ", cdf(quantile)=" + back);
        }
        assertEquals(0.0, d.cdf(-1.0));
        assertEquals(1.0, d.cdf(1e9));
    }

    @Test
    void weightedAdd() {
        TsTDigest d = new TsTDigest(100.0);
        d.addWeighted(10.0, 100.0);
        d.addWeighted(20.0, 100.0);
        assertEquals(200.0, d.count());
        double med = d.quantile(0.5);
        assertTrue(med >= 10.0 && med <= 20.0);
    }

    @Test
    void mergeMatchesCombined() {
        TsTDigest a = new TsTDigest(200.0);
        TsTDigest b = new TsTDigest(200.0);
        TsTDigest combined = new TsTDigest(200.0);
        for (int i = 0; i < 50_000; i++) {
            a.add(i);
            combined.add(i);
        }
        for (int i = 50_000; i < 100_000; i++) {
            b.add(i);
            combined.add(i);
        }
        TsTDigest m = a.merge(b);
        assertEquals(100_000.0, m.count());
        for (double q : new double[] {0.1, 0.5, 0.9, 0.99}) {
            double diff = Math.abs(m.quantile(q) - combined.quantile(q));
            assertTrue(diff < 500.0,
                    "q=" + q + ": merge=" + m.quantile(q) + " combined=" + combined.quantile(q) + " diff=" + diff);
        }
    }

    @Test
    void centroidCountBounded() {
        TsTDigest d = new TsTDigest(100.0);
        for (int i = 0; i < 1_000_000; i++) {
            d.add(i % 1000);
        }
        byte[] bytes = d.serialize();
        int centroids = (bytes.length - 29) / 16;
        assertTrue(centroids < 1_000, "centroids=" + centroids + " should be bounded");
    }

    @Test
    void serializeRoundtripByteIdentical() {
        TsTDigest d = new TsTDigest(150.0);
        for (int i = 0; i < 20_000; i++) {
            d.add(Math.sin(i * 0.001) * 100.0);
        }
        byte[] bytes = d.serialize();
        TsTDigest back = TsTDigest.deserialize(bytes);
        assertEquals(d.count(), back.count());
        for (double q : new double[] {0.01, 0.25, 0.5, 0.75, 0.99}) {
            assertTrue(Math.abs(back.quantile(q) - d.quantile(q)) < 1e-9);
        }
        assertArrayEquals(bytes, back.serialize());
    }

    @Test
    void deserializeBadVersion() {
        TsTDigest d = new TsTDigest(100.0);
        d.add(1.0);
        byte[] bytes = d.serialize();
        bytes[0] = 9;
        TsTDigestException ex = assertThrows(TsTDigestException.class, () -> TsTDigest.deserialize(bytes));
        assertEquals(TsTDigestException.Kind.BAD_VERSION, ex.kind());
        assertEquals(9, ex.version());
    }

    @Test
    void deserializeTruncated() {
        assertThrows(TsTDigestException.class, () -> TsTDigest.deserialize(new byte[0]));
        TsTDigest d = new TsTDigest(100.0);
        d.add(1.0);
        d.add(2.0);
        byte[] bytes = d.serialize();
        byte[] cut = Arrays.copyOf(bytes, bytes.length - 4);
        TsTDigestException ex = assertThrows(TsTDigestException.class, () -> TsTDigest.deserialize(cut));
        assertEquals(TsTDigestException.Kind.TRUNCATED, ex.kind());
    }

    // Canonical cross-language fixture: a Rust-serialized digest of values
    // 1.0..=10.0 at compression 100. Java MUST decode it to identical
    // centroids. Decode is byte-exact; same-input re-encode across languages
    // is not guaranteed bit-identical (libm asin / merge ordering), but a
    // deserialize -> re-serialize of these exact bytes round-trips identical.
    private static final String CANONICAL_HEX =
            "010000000000005940000000000000f03f00000000000024400a00000000"
            + "0000000000f03f000000000000f03f0000000000000040000000000000f03f"
            + "0000000000000840000000000000f03f0000000000001040000000000000f03f"
            + "0000000000001440000000000000f03f0000000000001840000000000000f03f"
            + "0000000000001c40000000000000f03f0000000000002040000000000000f03f"
            + "0000000000002240000000000000f03f0000000000002440000000000000f03f";

    @Test
    void decodesCanonicalRustFixture() {
        byte[] bytes = fromHex(CANONICAL_HEX);
        TsTDigest d = TsTDigest.deserialize(bytes);

        assertEquals(10.0, d.count());
        assertEquals(5.5, d.quantile(0.5));
        assertEquals(9.5, d.quantile(0.9));

        // Decode the header + centroids directly from the same bytes to assert
        // 10 centroids, means 1.0..10.0 each weight 1.0, min 1.0, max 10.0.
        ByteBuffer buf = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        assertEquals(1, buf.get());
        assertEquals(100.0, buf.getDouble());
        assertEquals(1.0, buf.getDouble());
        assertEquals(10.0, buf.getDouble());
        assertEquals(10, buf.getInt());
        for (int i = 0; i < 10; i++) {
            assertEquals((double) (i + 1), buf.getDouble(), "mean " + i);
            assertEquals(1.0, buf.getDouble(), "weight " + i);
        }

        // Round-trip of these exact bytes is byte-identical (pure f64 LE I/O).
        assertArrayEquals(bytes, d.serialize());
    }

    @Test
    void compressionFloor() {
        TsTDigest d = new TsTDigest(5.0);
        assertEquals(20.0, d.compression());
        assertNotEquals(5.0, d.compression());
    }
}
