package com.submillisecond.recipes.tsyaml;

import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Locale;
import java.util.Map;

import org.yaml.snakeyaml.LoaderOptions;
import org.yaml.snakeyaml.Yaml;
import org.yaml.snakeyaml.error.YAMLException;

import com.submillisecond.recipes.ts.TsCodec;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsTimestampStyle;

/**
 * Human-readable YAML codec for {@code TsSeries<Double>}. Implements the
 * {@link TsCodec} substrate from {@code subms-ts} with a clean, multi-line
 * columnar layout: two block sequences, {@code timestamps:} and {@code values:},
 * under a {@code subms_ts_series} document root.
 *
 * <p>This is a {@code category: adapter} recipe. Encoding is hand-written so the
 * output stays a tidy, diff-friendly columnar block; decoding goes through the
 * snakeyaml parser, because parsing arbitrary YAML back into a series is where a
 * real parser earns its place - block versus flow sequences, quoting, comments,
 * indentation, and the YAML core-schema scalar rules are thousands of lines of
 * corner cases we do not want to reimplement.
 *
 * <p>Timestamps render per a {@link TsTimestampStyle}, mirroring the JSON codec:
 * {@code EPOCH_NANOS} and {@code EPOCH_MILLIS} are integer columns that
 * round-trip; {@code ISO8601} is an encode-only rendering. Like the JSON and
 * CBOR codecs, the wire carries the data columns only; series metadata is not
 * part of the document. The emitted bytes are identical to the Rust port's.
 */
public final class TsYamlCodec implements TsCodec<Double> {

    private static final String ROOT_KEY = "subms_ts_series";
    private static final String TS_KEY = "timestamps";
    private static final String VAL_KEY = "values";

    private TsTimestampStyle style = TsTimestampStyle.EPOCH_NANOS;

    public TsYamlCodec() {
    }

    public TsYamlCodec withStyle(TsTimestampStyle style) {
        this.style = style;
        return this;
    }

    @Override
    public String format() {
        return "yaml";
    }

    @Override
    public byte[] encode(TsSeries<Double> series) {
        // Emitted by hand rather than through snakeyaml's dumper: the columnar
        // block layout is fully under our control and we want the tidy two-list
        // shape, not whatever flow/block heuristic a general emitter picks.
        int n = series.size();
        StringBuilder out = new StringBuilder(64 + n * 24);
        out.append(ROOT_KEY).append(":\n");
        out.append("  ").append(TS_KEY).append(':');
        if (n == 0) {
            out.append(" []\n");
        } else {
            out.append('\n');
            for (TsPoint<Double> p : series) {
                out.append("  - ").append(tsToken(p.ts())).append('\n');
            }
        }
        out.append("  ").append(VAL_KEY).append(':');
        if (n == 0) {
            out.append(" []\n");
        } else {
            out.append('\n');
            for (TsPoint<Double> p : series) {
                out.append("  - ").append(fmtDouble(p.value())).append('\n');
            }
        }
        return out.toString().getBytes(StandardCharsets.UTF_8);
    }

    @Override
    public TsSeries<Double> decode(byte[] bytes) {
        long scale;
        switch (style) {
            case EPOCH_NANOS -> scale = 1L;
            case EPOCH_MILLIS -> scale = 1_000_000L;
            default -> throw TsYamlException.unsupportedTimestampDecode();
        }

        String text = new String(bytes, StandardCharsets.UTF_8);
        Object root;
        try {
            Yaml yaml = new Yaml(new LoaderOptions());
            root = yaml.load(text);
        } catch (YAMLException e) {
            throw TsYamlException.parse(e.getMessage());
        }
        if (!(root instanceof Map<?, ?> doc)) {
            throw TsYamlException.parse("document root is not a mapping");
        }
        Object body = doc.get(ROOT_KEY);
        if (!(body instanceof Map<?, ?> inner)) {
            throw TsYamlException.parse("missing `" + ROOT_KEY + "` mapping");
        }

        long[] ts = readLongSeq(inner.get(TS_KEY), TS_KEY);
        double[] vals = readDoubleSeq(inner.get(VAL_KEY), VAL_KEY);
        if (ts.length != vals.length) {
            throw TsYamlException.parse(
                    "timestamps (" + ts.length + ") and values (" + vals.length + ") length mismatch");
        }

        TsSeries<Double> s = TsSeries.withCapacity(ts.length);
        for (int i = 0; i < ts.length; i++) {
            try {
                s.push(ts[i] * scale, vals[i]);
            } catch (RuntimeException e) {
                throw TsYamlException.parse(e.getMessage());
            }
        }
        return s;
    }

    private String tsToken(long ts) {
        return switch (style) {
            case EPOCH_NANOS -> Long.toString(ts);
            case EPOCH_MILLIS -> Long.toString(ts / 1_000_000L);
            case ISO8601 -> "\"" + iso8601FromNanos(ts) + "\"";
        };
    }

    private static long[] readLongSeq(Object node, String key) {
        if (!(node instanceof List<?> seq)) {
            throw TsYamlException.parse("`" + key + "` is not a sequence");
        }
        long[] out = new long[seq.size()];
        for (int i = 0; i < out.length; i++) {
            Object item = seq.get(i);
            if (item instanceof Integer iv) {
                out[i] = iv.longValue();
            } else if (item instanceof Long lv) {
                out[i] = lv;
            } else if (item instanceof java.math.BigInteger bv) {
                out[i] = bv.longValueExact();
            } else {
                throw TsYamlException.parse("`" + key + "` item is not an integer");
            }
        }
        return out;
    }

    private static double[] readDoubleSeq(Object node, String key) {
        if (!(node instanceof List<?> seq)) {
            throw TsYamlException.parse("`" + key + "` is not a sequence");
        }
        double[] out = new double[seq.size()];
        for (int i = 0; i < out.length; i++) {
            Object item = seq.get(i);
            // A whole-number value may parse as a YAML integer (`3`) rather than
            // a float (`3.0`); accept either so a hand-edited document decodes.
            if (item instanceof Number num) {
                out[i] = num.doubleValue();
            } else {
                throw TsYamlException.parse("`" + key + "` item is not a number");
            }
        }
        return out;
    }

    // Values are finite (push rejects NaN/inf). A whole number still prints with
    // a fractional part so the column stays unambiguously float, matching the
    // Rust port's rendering byte-for-byte.
    static String fmtDouble(double v) {
        if (v == Math.rint(v) && !Double.isInfinite(v) && Math.abs(v) < 1e15) {
            return String.format(Locale.ROOT, "%.1f", v);
        }
        return Double.toString(v);
    }

    // Civil date from epoch-nanoseconds, formatted ISO-8601 UTC. Hand-rolled
    // (Hinnant's algorithm) to match the Rust codec byte-for-byte.
    static String iso8601FromNanos(long ns) {
        long secs = Math.floorDiv(ns, 1_000_000_000L);
        long subNs = Math.floorMod(ns, 1_000_000_000L);
        long days = Math.floorDiv(secs, 86_400L);
        long secsOfDay = Math.floorMod(secs, 86_400L);
        long[] ymd = civilFromDays(days);
        long hh = secsOfDay / 3_600;
        long mm = (secsOfDay % 3_600) / 60;
        long ss = secsOfDay % 60;
        return String.format(Locale.ROOT,
                "%04d-%02d-%02dT%02d:%02d:%02d.%09dZ", ymd[0], ymd[1], ymd[2], hh, mm, ss, subNs);
    }

    static long[] civilFromDays(long z) {
        z += 719_468;
        long era = (z >= 0 ? z : z - 146_096) / 146_097;
        long doe = z - era * 146_097;
        long yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        long y = yoe + era * 400;
        long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        long mp = (5 * doy + 2) / 153;
        long d = doy - (153 * mp + 2) / 5 + 1;
        long m = mp < 10 ? mp + 3 : mp - 9;
        return new long[] {m <= 2 ? y + 1 : y, m, d};
    }
}
