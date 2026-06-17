package com.submillisecond.recipes.gorillablock;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

final class TsGorillaBlockTest {

    private record P(long ts, double value) {
    }

    private static List<P> roundtrip(List<P> points) {
        TsGorillaBlock b = new TsGorillaBlock();
        for (P p : points) {
            b.append(p.ts(), p.value());
        }
        byte[] bytes = b.bytes();
        TsGorillaBlock decoded = TsGorillaBlock.fromBytes(bytes);
        List<P> out = new ArrayList<>();
        for (TsPoint<Double> p : decoded) {
            out.add(new P(p.ts(), p.value()));
        }
        return out;
    }

    private static void assertBitExact(List<P> expected, List<P> got) {
        assertEquals(expected.size(), got.size(), "point count");
        for (int i = 0; i < expected.size(); i++) {
            assertEquals(expected.get(i).ts(), got.get(i).ts(), "ts at " + i);
            assertEquals(
                    Double.doubleToLongBits(expected.get(i).value()),
                    Double.doubleToLongBits(got.get(i).value()),
                    "value bits at " + i);
        }
    }

    private static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            sb.append(Character.forDigit((b >> 4) & 0xf, 16));
            sb.append(Character.forDigit(b & 0xf, 16));
        }
        return sb.toString();
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

    // Rust-encoded block of exactly these 5 points; the cross-language gate.
    private static final String FIXTURE_HEX =
            "010500000000000000000003e83ff0000000000000e3e8c25fffec1871f46133fffc0000000000005dc301fffcc0";
    private static final List<P> FIXTURE_POINTS = List.of(
            new P(1000, 1.0),
            new P(2000, 2.0),
            new P(2000, 2.0),
            new P(3000, 1.5),
            new P(10000, -7.5));

    @Test
    void canonicalFixtureDecodes() {
        List<TsPoint<Double>> got = TsGorillaBlock.decode(fromHex(FIXTURE_HEX));
        assertEquals(FIXTURE_POINTS.size(), got.size());
        for (int i = 0; i < FIXTURE_POINTS.size(); i++) {
            assertEquals(FIXTURE_POINTS.get(i).ts(), got.get(i).ts(), "ts at " + i);
            assertEquals(
                    Double.doubleToLongBits(FIXTURE_POINTS.get(i).value()),
                    Double.doubleToLongBits(got.get(i).value()),
                    "value bits at " + i);
        }
    }

    @Test
    void canonicalFixtureReencodes() {
        TsGorillaBlock b = new TsGorillaBlock();
        for (P p : FIXTURE_POINTS) {
            b.append(p.ts(), p.value());
        }
        assertEquals(FIXTURE_HEX, toHex(b.bytes()), "Java re-encode must match the Rust wire bytes");
    }

    @Test
    void emptyBlock() {
        TsGorillaBlock b = new TsGorillaBlock();
        assertTrue(b.isEmpty());
        assertEquals(0, b.len());
        int n = 0;
        for (TsPoint<Double> ignored : b) {
            n++;
        }
        assertEquals(0, n);
        byte[] bytes = b.bytes();
        assertEquals(0, TsGorillaBlock.fromBytes(bytes).len());
        assertTrue(TsGorillaBlock.decode(new byte[0]).isEmpty());
    }

    @Test
    void singlePoint() {
        List<P> pts = List.of(new P(1_700_000_000_000L, 123.456));
        assertBitExact(pts, roundtrip(pts));
    }

    @Test
    void constantValueXorZeroPath() {
        List<P> pts = new ArrayList<>();
        for (long i = 0; i < 500; i++) {
            pts.add(new P(i, 42.0));
        }
        assertBitExact(pts, roundtrip(pts));
    }

    @Test
    void constantIntervalDodZeroPath() {
        List<P> pts = new ArrayList<>();
        for (long i = 0; i < 500; i++) {
            pts.add(new P(i * 1_000, i * 0.5));
        }
        assertBitExact(pts, roundtrip(pts));
    }

    @Test
    void irregularIntervalsAndLargeJumps() {
        List<P> pts = List.of(
                new P(0, 1.0),
                new P(5, 1.5),
                new P(5_000_000, 2.0),       // big jump -> 64-bit dod fallback
                new P(5_000_063, 2.0),       // small dod
                new P(10_000_000_000L, -7.5)); // huge jump
        assertBitExact(pts, roundtrip(pts));
    }

    @Test
    void negativeAndMixedValues() {
        List<P> pts = List.of(
                new P(1, -1.0),
                new P(2, -2.5),
                new P(3, 0.0),
                new P(4, 1e-9),
                new P(5, -1e9),
                new P(6, 123456.789));
        assertBitExact(pts, roundtrip(pts));
    }

    @Test
    void randomWalkBitExact() {
        long state = 88172645463325252L;
        double v = 100.0;
        List<P> pts = new ArrayList<>();
        for (long i = 0; i < 2_000; i++) {
            state ^= state << 13;
            state ^= state >>> 7;
            state ^= state << 17;
            v += ((state >>> 40) / (double) 0xffffffffL) - 0.5;
            pts.add(new P(i * 1_000 + (state & 7), v));
        }
        assertBitExact(pts, roundtrip(pts));
    }

    @Test
    void reencodeIsDeterministic() {
        TsGorillaBlock b = new TsGorillaBlock();
        for (long i = 0; i < 1_000; i++) {
            b.append(i * 1_000, Math.sin(i * 0.01));
        }
        byte[] bytes1 = b.bytes();
        byte[] bytes2 = TsGorillaBlock.fromBytes(bytes1).bytes();
        assertEquals(toHex(bytes1), toHex(bytes2), "from_bytes round-trip must be byte-identical");
    }

    @Test
    void compressesConstantValueHard() {
        TsGorillaBlock b = new TsGorillaBlock();
        for (long i = 0; i < 4_096; i++) {
            b.append(1_700_000_000L + i, 42.0);
        }
        double perPoint = b.bytes().length / 4_096.0;
        assertTrue(perPoint < 1.0, "constant series: " + perPoint + " bytes/point");
    }

    @Test
    void compressesSteppedGauge() {
        TsGorillaBlock b = new TsGorillaBlock();
        for (long i = 0; i < 4_096; i++) {
            double v = 20.0 + (i / 16);
            b.append(1_700_000_000L + i, v);
        }
        double perPoint = b.bytes().length / 4_096.0;
        assertTrue(perPoint < 4.0, "stepped gauge: " + perPoint + " bytes/point");
    }

    @Test
    void rangeFilter() {
        TsGorillaBlock b = new TsGorillaBlock();
        for (long i = 0; i < 100; i++) {
            b.append(i, (double) i);
        }
        List<Long> got = new ArrayList<>();
        for (TsPoint<Double> p : b.range(40, 45)) {
            got.add(p.ts());
        }
        assertEquals(List.of(40L, 41L, 42L, 43L, 44L, 45L), got);
        assertEquals(0, b.range(200, 300).size());
    }

    @Test
    void mergeOrdersPoints() {
        TsGorillaBlock a = new TsGorillaBlock();
        for (long i = 0; i < 50; i++) {
            a.append(i, (double) i);
        }
        TsGorillaBlock c = new TsGorillaBlock();
        for (long i = 50; i < 100; i++) {
            c.append(i, (double) i);
        }
        TsGorillaBlock m = a.merge(c);
        assertEquals(100, m.len());
        List<Long> ts = new ArrayList<>();
        for (TsPoint<Double> p : m) {
            ts.add(p.ts());
        }
        List<Long> expected = new ArrayList<>();
        for (long i = 0; i < 100; i++) {
            expected.add(i);
        }
        assertEquals(expected, ts);
    }

    @Test
    void statsTrackExtremes() {
        TsGorillaBlock b = new TsGorillaBlock();
        b.append(10, 5.0);
        b.append(20, 1.0);
        b.append(30, 9.0);
        b.append(40, 3.0);
        TsBlockStats s = b.stats();
        assertEquals(4, s.count());
        assertEquals(10, s.tsMin());
        assertEquals(40, s.tsMax());
        assertEquals(1.0, s.valueMin());
        assertEquals(9.0, s.valueMax());
    }

    @Test
    void statsEmptyBlock() {
        TsBlockStats s = new TsGorillaBlock().stats();
        assertEquals(0, s.count());
        assertEquals(0.0, s.valueMin());
        assertEquals(0.0, s.valueMax());
    }

    @Test
    void badVersionRejected() {
        TsGorillaBlock b = new TsGorillaBlock();
        b.append(1, 1.0);
        byte[] raw = b.bytes();
        raw[0] = 99;
        TsBlockException ex = assertThrows(TsBlockException.class, () -> TsGorillaBlock.fromBytes(raw));
        assertEquals(TsBlockException.Kind.BAD_VERSION, ex.kind());
        assertEquals(99, ex.version());
    }

    @Test
    void truncatedRejected() {
        TsGorillaBlock b = new TsGorillaBlock();
        for (long i = 0; i < 20; i++) {
            b.append(i, (double) i);
        }
        byte[] raw = b.bytes();
        byte[] cut = new byte[7];
        System.arraycopy(raw, 0, cut, 0, 7);
        TsBlockException ex = assertThrows(TsBlockException.class, () -> TsGorillaBlock.fromBytes(cut));
        assertEquals(TsBlockException.Kind.TRUNCATED, ex.kind());
    }

    @Test
    void codecRoundtrips() {
        TsGorillaCodec codec = new TsGorillaCodec();
        assertEquals("gorilla", codec.format());
        TsSeries<Double> s = TsSeries.withCapacity(64);
        for (long i = 0; i < 64; i++) {
            s.push(i * 1_000, 20.0 + Math.sin(i * 0.1));
        }
        byte[] bytes = codec.encode(s);
        TsSeries<Double> back = codec.decode(bytes);
        assertEquals(s.size(), back.size());
        List<TsPoint<Double>> a = new ArrayList<>();
        for (TsPoint<Double> p : s) {
            a.add(p);
        }
        List<TsPoint<Double>> c = new ArrayList<>();
        for (TsPoint<Double> p : back) {
            c.add(p);
        }
        for (int i = 0; i < a.size(); i++) {
            assertEquals(a.get(i).ts(), c.get(i).ts());
            assertEquals(Double.doubleToLongBits(a.get(i).value()), Double.doubleToLongBits(c.get(i).value()));
        }
    }

    @Test
    void withCapacityAppendable() {
        TsGorillaBlock b = TsGorillaBlock.withCapacity(8);
        assertFalse(b.len() > 0);
        for (long i = 0; i < 300; i++) {
            b.append(i, (double) i);
        }
        assertEquals(300, b.len());
        assertEquals(300, TsGorillaBlock.decode(b.bytes()).size());
    }

    @Test
    void mmapDecodeMatchesInMemory() throws java.io.IOException {
        TsGorillaBlock b = new TsGorillaBlock();
        for (int i = 0; i < 2048; i++) {
            b.append(1_000L + i * 1_000L, 42.0 + Math.sin(i * 0.01));
        }
        java.nio.file.Path path = java.nio.file.Files.createTempFile("subms_gorilla_mmap", ".blk");
        try {
            java.nio.file.Files.write(path, b.bytes());

            List<TsPoint<Double>> viaMmap = TsGorillaBlock.decodeMmap(path);
            List<TsPoint<Double>> inMem = TsGorillaBlock.decode(b.bytes());
            assertEquals(inMem.size(), viaMmap.size());
            for (int i = 0; i < inMem.size(); i++) {
                assertEquals(inMem.get(i).ts(), viaMmap.get(i).ts());
                assertEquals(inMem.get(i).value(), viaMmap.get(i).value());
            }

            TsGorillaBlock rebuilt = TsGorillaBlock.fromMmap(path);
            assertEquals(2048, rebuilt.len());
        } finally {
            java.nio.file.Files.deleteIfExists(path);
        }
    }
}
