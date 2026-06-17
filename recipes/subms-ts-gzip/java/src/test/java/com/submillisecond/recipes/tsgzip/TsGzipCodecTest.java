package com.submillisecond.recipes.tsgzip;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsCodec;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

class TsGzipCodecTest {

    private static TsGzipCodec<Double> codec() {
        return new TsGzipCodec<>(new JsonInnerCodec(), 6);
    }

    private static TsSeries<Double> series(double[][] pts) {
        TsSeries<Double> s = new TsSeries<>();
        for (double[] p : pts) {
            s.push((long) p[0], p[1]);
        }
        return s;
    }

    private static TsSeries<Double> repetitive(int n) {
        TsSeries<Double> s = new TsSeries<>();
        for (long i = 0; i < n; i++) {
            s.push(i * 10, (double) (i % 8));
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
    void roundTripBasic() {
        TsSeries<Double> s = series(new double[][] {{1, 1.5}, {2, 2.5}, {3, 3.5}});
        TsGzipCodec<Double> c = codec();
        assertSamePairs(s, c.decode(c.encode(s)));
    }

    @Test
    void roundTripEmpty() {
        TsSeries<Double> s = new TsSeries<>();
        TsGzipCodec<Double> c = codec();
        assertTrue(c.decode(c.encode(s)).isEmpty());
    }

    @Test
    void roundTripSinglePoint() {
        TsSeries<Double> s = series(new double[][] {{42, 1.25}});
        TsGzipCodec<Double> c = codec();
        assertSamePairs(s, c.decode(c.encode(s)));
    }

    @Test
    void roundTripNegativeAndLargeTs() {
        TsSeries<Double> s = series(new double[][] {
            {-1_000_000, -2.5}, {0, 0.0}, {9_000_000_000.0, 42.25}});
        TsGzipCodec<Double> c = codec();
        assertSamePairs(s, c.decode(c.encode(s)));
    }

    @Test
    void roundTripManyPoints() {
        long state = 0xabcd_1234L;
        TsSeries<Double> s = new TsSeries<>();
        for (long i = 0; i < 5_000; i++) {
            state = state * 6364136223846793005L + 1L;
            s.push(i, (double) (state >>> 11) / 13.0 - 100.0);
        }
        TsGzipCodec<Double> c = codec();
        assertSamePairs(s, c.decode(c.encode(s)));
    }

    @Test
    void valuesAreBitExact() {
        TsSeries<Double> s = series(new double[][] {
            {1, Math.PI}, {2, 1.0 / 3.0}, {3, 1e-300}});
        TsGzipCodec<Double> c = codec();
        TsSeries<Double> back = c.decode(c.encode(s));
        List<double[]> a = pairs(s);
        List<double[]> b = pairs(back);
        for (int i = 0; i < a.size(); i++) {
            assertEquals(Double.doubleToLongBits(a.get(i)[1]),
                    Double.doubleToLongBits(b.get(i)[1]), "bits at " + i);
        }
    }

    @Test
    void compressionActuallyShrinks() {
        TsSeries<Double> s = repetitive(5_000);
        byte[] raw = new JsonInnerCodec().encode(s);
        byte[] gz = codec().encode(s);
        assertTrue(gz.length < raw.length, "gzip " + gz.length + " < json " + raw.length);
        assertTrue(gz.length * 2 < raw.length, "expected >2x on repetitive data");
    }

    @Test
    void formatIsGzipJson() {
        assertEquals("gzip+json", codec().format());
    }

    @Test
    void allLevelsRoundTrip() {
        TsSeries<Double> s = repetitive(2_000);
        for (int level = 0; level <= 9; level++) {
            TsGzipCodec<Double> c = new TsGzipCodec<>(new JsonInnerCodec(), level);
            assertSamePairs(s, c.decode(c.encode(s)), level);
        }
    }

    private static void assertSamePairs(TsSeries<Double> a, TsSeries<Double> b, int level) {
        assertEquals(pairs(a).size(), pairs(b).size(), "row count at level " + level);
        assertSamePairs(a, b);
    }

    @Test
    void storedBlockRoundTrips() {
        TsSeries<Double> s = series(new double[][] {{1, 1.0}, {2, 2.0}, {3, 3.0}});
        TsGzipCodec<Double> c = new TsGzipCodec<>(new JsonInnerCodec(), 0);
        assertSamePairs(s, c.decode(c.encode(s)));
    }

    @Test
    void rawGzipGunzipRoundTrip() {
        byte[] payload = "the quick brown fox jumps over the lazy dog, again and again and again"
                .getBytes(java.nio.charset.StandardCharsets.UTF_8);
        byte[] gz = TsGzipCodec.gzip(payload, 6);
        assertEquals(0x1f, gz[0] & 0xff);
        assertEquals(0x8b, gz[1] & 0xff);
        assertArrayEquals(payload, TsGzipCodec.gunzip(gz));
    }

    // ---- error cases ----

    @Test
    void decodeRejectsBadMagic() {
        byte[] gz = codec().encode(series(new double[][] {{1, 1.0}}));
        gz[0] = 0x00;
        TsGzipException e = assertThrows(TsGzipException.class, () -> codec().decode(gz));
        assertEquals(TsGzipException.Kind.BAD_MAGIC, e.kind());
    }

    @Test
    void decodeRejectsTruncated() {
        byte[] gz = codec().encode(series(new double[][] {{1, 1.0}, {2, 2.0}}));
        byte[] cut = java.util.Arrays.copyOf(gz, gz.length - 4);
        // A short trailer trips Truncated or a CRC/size check; all are gzip-layer.
        assertThrows(TsGzipException.class, () -> codec().decode(cut));
        TsGzipException empty = assertThrows(TsGzipException.class,
                () -> codec().decode(new byte[0]));
        assertEquals(TsGzipException.Kind.TRUNCATED, empty.kind());
    }

    @Test
    void decodeRejectsCorruptCrc() {
        byte[] gz = codec().encode(repetitive(200));
        gz[gz.length - 8] ^= 0xff; // flip a CRC byte
        TsGzipException e = assertThrows(TsGzipException.class, () -> codec().decode(gz));
        assertEquals(TsGzipException.Kind.CRC_MISMATCH, e.kind());
    }

    @Test
    void decodeRejectsBadMethod() {
        byte[] gz = codec().encode(series(new double[][] {{1, 1.0}}));
        gz[2] = 9; // CM=9, not deflate
        TsGzipException e = assertThrows(TsGzipException.class, () -> codec().decode(gz));
        assertEquals(TsGzipException.Kind.BAD_METHOD, e.kind());
    }

    // ---- interop: oracle is the system `gzip` / `gunzip` (gzip 1.14) ----

    // Resolve a gzip-family tool: prefer PATH (CI), fall back to common Windows
    // install locations where the binary exists but is not on the Windows PATH.
    private static String resolve(String cmd) {
        String[] candidates = {
            "C:\\Program Files\\Git\\usr\\bin\\" + cmd + ".exe",
            "C:\\Program Files\\Git\\bin\\" + cmd + ".exe",
            "/usr/bin/" + cmd
        };
        for (String c : candidates) {
            if (new java.io.File(c).isFile()) {
                return c;
            }
        }
        return cmd; // rely on PATH (CI runners have gzip installed)
    }

    private static byte[] runPipe(String cmd, String[] args, byte[] input) {
        try {
            List<String> cmdline = new ArrayList<>();
            cmdline.add(resolve(cmd));
            for (String a : args) {
                cmdline.add(a);
            }
            Process p = new ProcessBuilder(cmdline).redirectErrorStream(false).start();
            Thread writer = new Thread(() -> {
                try (var os = p.getOutputStream()) {
                    os.write(input);
                } catch (IOException ignored) {
                    // child may close early; the exit code is the source of truth.
                }
            });
            writer.start();
            byte[] out = p.getInputStream().readAllBytes();
            p.getErrorStream().readAllBytes();
            int code = p.waitFor();
            writer.join();
            return code == 0 ? out : null;
        } catch (IOException | InterruptedException e) {
            if (e instanceof InterruptedException) {
                Thread.currentThread().interrupt();
            }
            return null;
        }
    }

    // gunzip is often an MSYS shell builtin; `gzip -d -c` is the portable oracle.
    private static byte[] toolDecompress(byte[] input) {
        byte[] r = runPipe("gunzip", new String[] {"-c"}, input);
        return r != null ? r : runPipe("gzip", new String[] {"-d", "-c"}, input);
    }

    // Oracle (a): our gzip output is decodable by the system tool.
    @Test
    void interopOurOutputGunzips() {
        TsSeries<Double> s = repetitive(5_000);
        byte[] expected = new JsonInnerCodec().encode(s);
        byte[] gz = codec().encode(s);
        byte[] decoded = toolDecompress(gz);
        org.junit.jupiter.api.Assumptions.assumeTrue(decoded != null, "system gzip not on PATH");
        assertArrayEquals(expected, decoded, "system gunzip of our output equals inner json bytes");
    }

    // Oracle (b): we INFLATE the tool's (dynamic-Huffman) output.
    @Test
    void interopWeInflateToolOutput() {
        TsSeries<Double> s = repetitive(5_000);
        byte[] payload = new JsonInnerCodec().encode(s);
        byte[] toolGz = runPipe("gzip", new String[] {"-9", "-c"}, payload);
        org.junit.jupiter.api.Assumptions.assumeTrue(toolGz != null, "system gzip not on PATH");
        assertArrayEquals(payload, TsGzipCodec.gunzip(toolGz), "our gunzip of system gzip round-trips");
        // And via the full codec.decode path.
        assertSamePairs(s, codec().decode(toolGz));
    }
}
