package com.submillisecond.recipes.tsyaml;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;
import org.yaml.snakeyaml.Yaml;

import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsTimestampStyle;

class TsYamlCodecTest {

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

    private static String doc(byte[] bytes) {
        return new String(bytes, StandardCharsets.UTF_8);
    }

    @Test
    void roundTripBasic() {
        TsSeries<Double> s = series(new double[][] {{1, 1.5}, {2, 2.5}, {3, 3.5}});
        TsYamlCodec codec = new TsYamlCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void roundTripEmpty() {
        TsSeries<Double> s = new TsSeries<>();
        TsYamlCodec codec = new TsYamlCodec();
        byte[] bytes = codec.encode(s);
        TsSeries<Double> back = codec.decode(bytes);
        assertTrue(back.isEmpty());
    }

    @Test
    void roundTripNegativeAndLargeTs() {
        TsSeries<Double> s = series(new double[][] {
            {-1_000_000, -2.5}, {0, 0.0}, {9_000_000_000.0, 42.25}});
        TsYamlCodec codec = new TsYamlCodec();
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
        TsYamlCodec codec = new TsYamlCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void valuesRoundTripBitExact() {
        TsSeries<Double> s = series(new double[][] {
            {1, Math.PI}, {2, 1.0 / 3.0}, {3, 1e-300}});
        TsYamlCodec codec = new TsYamlCodec();
        TsSeries<Double> back = codec.decode(codec.encode(s));
        List<double[]> a = pairs(s);
        List<double[]> b = pairs(back);
        for (int i = 0; i < a.size(); i++) {
            assertEquals(Double.doubleToLongBits(a.get(i)[1]),
                    Double.doubleToLongBits(b.get(i)[1]), "bits at " + i);
        }
    }

    @Test
    void encodeIsColumnarBlockLayout() {
        // Pins the cross-language wire layout: points (1,1.0) (2,2.0). The Rust
        // port asserts the identical document text.
        TsSeries<Double> s = series(new double[][] {{1, 1.0}, {2, 2.0}});
        assertEquals(
                "subms_ts_series:\n  timestamps:\n  - 1\n  - 2\n  values:\n  - 1.0\n  - 2.0\n",
                doc(new TsYamlCodec().encode(s)));
    }

    @Test
    void encodeIsValidYaml() {
        // Re-parse the emitted document with snakeyaml to confirm it is
        // well-formed YAML, independent of our own decode path.
        TsSeries<Double> s = series(new double[][] {{10, 1.5}, {20, 2.5}});
        Object root = new Yaml().load(doc(new TsYamlCodec().encode(s)));
        Map<?, ?> top = (Map<?, ?>) root;
        Map<?, ?> inner = (Map<?, ?>) top.get("subms_ts_series");
        assertEquals(2, ((List<?>) inner.get("timestamps")).size());
        assertEquals(2, ((List<?>) inner.get("values")).size());
    }

    @Test
    void encodeEpochNanosIsRawInteger() {
        TsSeries<Double> s = series(new double[][] {{1_500_000_000.0, 7.0}});
        String out = doc(new TsYamlCodec().withStyle(TsTimestampStyle.EPOCH_NANOS).encode(s));
        assertTrue(out.contains("- 1500000000\n"), out);
    }

    @Test
    void encodeEpochMillisScalesDown() {
        TsSeries<Double> s = series(new double[][] {{1_500_000_000.0, 7.0}});
        String out = doc(new TsYamlCodec().withStyle(TsTimestampStyle.EPOCH_MILLIS).encode(s));
        assertTrue(out.contains("- 1500\n"), out);
    }

    @Test
    void epochMillisRoundTripsThroughNanos() {
        TsSeries<Double> s = series(new double[][] {
            {1_500_000_000.0, 7.0}, {3_000_000_000.0, 9.0}});
        TsYamlCodec codec = new TsYamlCodec().withStyle(TsTimestampStyle.EPOCH_MILLIS);
        TsSeries<Double> back = codec.decode(codec.encode(s));
        assertSamePairs(s, back);
    }

    @Test
    void encodeIso8601RendersTimestampStrings() {
        TsSeries<Double> s = series(new double[][] {{0, 1.0}});
        String out = doc(new TsYamlCodec().withStyle(TsTimestampStyle.ISO8601).encode(s));
        assertTrue(out.contains("1970-01-01T00:00:00.000000000Z"), out);
    }

    @Test
    void decodeIso8601IsUnsupported() {
        TsSeries<Double> s = series(new double[][] {{0, 1.0}});
        TsYamlCodec codec = new TsYamlCodec().withStyle(TsTimestampStyle.ISO8601);
        byte[] bytes = codec.encode(s);
        TsYamlException e = assertThrows(TsYamlException.class, () -> codec.decode(bytes));
        assertEquals(TsYamlException.Kind.UNSUPPORTED_TIMESTAMP_DECODE, e.kind());
    }

    @Test
    void decodeAcceptsWholeNumberValuesAsIntegers() {
        String d = "subms_ts_series:\n  timestamps:\n  - 1\n  - 2\n  values:\n  - 3\n  - 4\n";
        TsSeries<Double> back = new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8));
        List<double[]> got = pairs(back);
        assertEquals(2, got.size());
        assertEquals(3.0, got.get(0)[1]);
        assertEquals(4.0, got.get(1)[1]);
    }

    @Test
    void decodeAcceptsFlowSequences() {
        String d = "subms_ts_series:\n  timestamps: [1, 2, 3]\n  values: [1.5, 2.5, 3.5]\n";
        TsSeries<Double> back = new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8));
        assertEquals(3, back.size());
    }

    @Test
    void decodeRejectsMalformedYaml() {
        // Unbalanced flow bracket - a syntax error snakeyaml catches.
        String d = "subms_ts_series:\n  timestamps: [1, 2\n  values: [1.0]\n";
        TsYamlException e = assertThrows(TsYamlException.class,
                () -> new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8)));
        assertEquals(TsYamlException.Kind.PARSE, e.kind());
    }

    @Test
    void decodeRejectsMissingRoot() {
        String d = "other_key:\n  timestamps: [1]\n  values: [1.0]\n";
        TsYamlException e = assertThrows(TsYamlException.class,
                () -> new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8)));
        assertEquals(TsYamlException.Kind.PARSE, e.kind());
    }

    @Test
    void decodeRejectsMissingValuesColumn() {
        String d = "subms_ts_series:\n  timestamps: [1, 2]\n";
        TsYamlException e = assertThrows(TsYamlException.class,
                () -> new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8)));
        assertEquals(TsYamlException.Kind.PARSE, e.kind());
    }

    @Test
    void decodeRejectsLengthMismatch() {
        String d = "subms_ts_series:\n  timestamps: [1, 2, 3]\n  values: [1.0, 2.0]\n";
        TsYamlException e = assertThrows(TsYamlException.class,
                () -> new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8)));
        assertEquals(TsYamlException.Kind.PARSE, e.kind());
    }

    @Test
    void decodeRejectsNonIntegerTimestamp() {
        String d = "subms_ts_series:\n  timestamps: [hello]\n  values: [1.0]\n";
        TsYamlException e = assertThrows(TsYamlException.class,
                () -> new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8)));
        assertEquals(TsYamlException.Kind.PARSE, e.kind());
    }

    @Test
    void decodeRejectsScalarInsteadOfSequence() {
        String d = "subms_ts_series:\n  timestamps: 1\n  values: [1.0]\n";
        TsYamlException e = assertThrows(TsYamlException.class,
                () -> new TsYamlCodec().decode(d.getBytes(StandardCharsets.UTF_8)));
        assertEquals(TsYamlException.Kind.PARSE, e.kind());
    }

    @Test
    void formatIsYaml() {
        assertEquals("yaml", new TsYamlCodec().format());
    }
}
