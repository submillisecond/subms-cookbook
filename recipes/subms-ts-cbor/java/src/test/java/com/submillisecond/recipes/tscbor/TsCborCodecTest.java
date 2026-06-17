package com.submillisecond.recipes.tscbor;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

class TsCborCodecTest {

    // Pins the cross-language wire layout: points (1,1.0) (2,2.0). The Rust port
    // asserts the identical hex.
    private static final String CBOR_FIXTURE =
            "a2627473820102617682fb3ff0000000000000fb4000000000000000";

    private static TsSeries<Double> series(double[][] pts) {
        TsSeries<Double> s = new TsSeries<>();
        for (double[] p : pts) {
            s.push((long) p[0], p[1]);
        }
        return s;
    }

    private static List<double[]> pairs(TsSeries<Double> s) {
        List<double[]> out = new ArrayList<>();
        for (TsPoint<Double> p : s) {
            out.add(new double[] {p.ts(), p.value()});
        }
        return out;
    }

    private static String toHex(byte[] b) {
        StringBuilder sb = new StringBuilder(b.length * 2);
        for (byte x : b) {
            sb.append(String.format("%02x", x & 0xff));
        }
        return sb.toString();
    }

    private static void assertSamePairs(TsSeries<Double> a, TsSeries<Double> b) {
        List<double[]> pa = pairs(a);
        List<double[]> pb = pairs(b);
        assertEquals(pa.size(), pb.size(), "row count");
        for (int i = 0; i < pa.size(); i++) {
            assertEquals((long) pa.get(i)[0], (long) pb.get(i)[0], "ts at " + i);
            assertEquals(Double.doubleToLongBits(pa.get(i)[1]),
                    Double.doubleToLongBits(pb.get(i)[1]), "value bits at " + i);
        }
    }

    @Test
    void encodeMatchesFixture() {
        TsSeries<Double> s = series(new double[][] {{1, 1.0}, {2, 2.0}});
        assertEquals(CBOR_FIXTURE, toHex(new TsCborCodec().encode(s)));
    }

    @Test
    void roundTripBasic() {
        TsSeries<Double> s = series(new double[][] {{1, 1.5}, {2, 2.5}, {3, 3.5}});
        TsCborCodec codec = new TsCborCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void roundTripEmpty() {
        TsSeries<Double> s = new TsSeries<>();
        TsCborCodec codec = new TsCborCodec();
        byte[] bytes = codec.encode(s);
        TsSeries<Double> back = codec.decode(bytes);
        assertTrue(back.isEmpty());
    }

    @Test
    void roundTripNegativeAndLargeTs() {
        TsSeries<Double> s = series(new double[][] {
            {-1_000_000, -2.5}, {0, 0.0}, {9_000_000_000.0, 42.25}});
        TsCborCodec codec = new TsCborCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void roundTripManyPoints() {
        long state = 0xabcd_1234L;
        TsSeries<Double> s = new TsSeries<>();
        for (long i = 0; i < 5_000; i++) {
            state = state * 6364136223846793005L + 1L;
            s.push(i, (double) (state >>> 11) / 13.0 - 100.0);
        }
        TsCborCodec codec = new TsCborCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void valuesAreBitExact() {
        TsSeries<Double> s = series(new double[][] {
            {1, Math.PI}, {2, 1.0 / 3.0}, {3, 1e-300}});
        TsCborCodec codec = new TsCborCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        List<double[]> a = pairs(s);
        List<double[]> b = pairs(back);
        for (int i = 0; i < a.size(); i++) {
            assertEquals(Double.doubleToLongBits(a.get(i)[1]),
                    Double.doubleToLongBits(b.get(i)[1]), "bits at " + i);
        }
    }

    @Test
    void wideIntegerHeadsRoundTrip() {
        // ts values that exercise every int head width (1/2/4/8 byte).
        TsSeries<Double> s = series(new double[][] {
            {10, 1.0}, {300, 2.0}, {70_000, 3.0}, {5_000_000_000.0, 4.0}});
        TsCborCodec codec = new TsCborCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void formatIsCbor() {
        assertEquals("cbor", new TsCborCodec().format());
    }

    @Test
    void decodeIsKeyOrderIndependent() {
        // Hand-build a map with "v" before "ts"; decode must still work.
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        buf.write(0xa2);
        // "v" -> array(1) -> f64 1.0
        buf.write(0x61);
        buf.write('v');
        buf.write(0x81);
        buf.write(0xfb);
        writeBe64(buf, Double.doubleToLongBits(1.0));
        // "ts" -> array(1) -> int 7
        buf.write(0x62);
        buf.write('t');
        buf.write('s');
        buf.write(0x81);
        buf.write(0x07);
        TsSeries<Double> back = new TsCborCodec().decode(buf.toByteArray());
        List<double[]> got = pairs(back);
        assertEquals(1, got.size());
        assertEquals(7L, (long) got.get(0)[0]);
        assertEquals(1.0, got.get(0)[1]);
    }

    @Test
    void decodeRejectsTruncated() {
        TsSeries<Double> s = series(new double[][] {{1, 1.0}, {2, 2.0}});
        byte[] full = new TsCborCodec().encode(s);
        byte[] cut = new byte[full.length - 3];
        System.arraycopy(full, 0, cut, 0, cut.length);
        TsCborException e = assertThrows(TsCborException.class,
                () -> new TsCborCodec().decode(cut));
        assertEquals(TsCborException.Kind.TRUNCATED, e.kind());
        TsCborException empty = assertThrows(TsCborException.class,
                () -> new TsCborCodec().decode(new byte[0]));
        assertEquals(TsCborException.Kind.TRUNCATED, empty.kind());
    }

    @Test
    void decodeRejectsNonMap() {
        // 0x82 = array(2), not a map.
        TsCborException e = assertThrows(TsCborException.class,
                () -> new TsCborCodec().decode(new byte[] {(byte) 0x82, 0x01, 0x02}));
        assertEquals(TsCborException.Kind.UNEXPECTED, e.kind());
    }

    @Test
    void decodeRejectsLengthMismatch() {
        // map(2): ts -> array(2) [1,2], v -> array(1) [1.0]
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        for (int b : new int[] {0xa2, 0x62, 't', 's', 0x82, 0x01, 0x02, 0x61, 'v', 0x81, 0xfb}) {
            buf.write(b);
        }
        writeBe64(buf, Double.doubleToLongBits(1.0));
        TsCborException e = assertThrows(TsCborException.class,
                () -> new TsCborCodec().decode(buf.toByteArray()));
        assertEquals(TsCborException.Kind.UNEXPECTED, e.kind());
    }

    @Test
    void decodeRejectsUnknownKey() {
        // map(2) with an unexpected key "x".
        byte[] buf = {(byte) 0xa2, 0x61, 'x', (byte) 0x80, 0x61, 'v', (byte) 0x80};
        TsCborException e = assertThrows(TsCborException.class,
                () -> new TsCborCodec().decode(buf));
        assertEquals(TsCborException.Kind.UNEXPECTED, e.kind());
    }

    private static void writeBe64(ByteArrayOutputStream out, long n) {
        for (int i = 7; i >= 0; i--) {
            out.write((int) (n >>> (i * 8)) & 0xff);
        }
    }
}
