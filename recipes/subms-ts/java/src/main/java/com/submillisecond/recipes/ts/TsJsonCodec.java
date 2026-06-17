package com.submillisecond.recipes.ts;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * Columnar JSON codec for the scalar-{@code double} fast path
 * ({@link TsSeriesD}): {@code {"name":..,"timestamps":[..],"values":[..]}}.
 * Zero-dependency, human-readable, round-trips for the epoch styles. ISO-8601
 * is encode-only in 0.6 (decoding it needs calendar parsing, deferred).
 */
public final class TsJsonCodec {

    private TsTimestampStyle style = TsTimestampStyle.EPOCH_NANOS;
    private boolean pretty = false;

    public TsJsonCodec() {}

    public TsJsonCodec withStyle(TsTimestampStyle style) {
        this.style = style;
        return this;
    }

    public TsJsonCodec pretty(boolean pretty) {
        this.pretty = pretty;
        return this;
    }

    public String format() {
        return "json";
    }

    public byte[] encode(TsSeriesD series) {
        String nl = pretty ? "\n" : "";
        String sp = pretty ? "  " : "";
        StringBuilder out = new StringBuilder();
        out.append('{').append(nl);
        series.metadata().ifPresent(m -> {
            out.append(sp).append("\"name\":").append(jsonString(m.name())).append(',').append(nl);
        });
        out.append(sp).append("\"timestamps\":[");
        List<TsPoint<Double>> points = series.toList();
        for (int i = 0; i < points.size(); i++) {
            if (i > 0) out.append(',');
            out.append(tsToken(points.get(i).ts()));
        }
        out.append("],").append(nl);
        out.append(sp).append("\"values\":[");
        for (int i = 0; i < points.size(); i++) {
            if (i > 0) out.append(',');
            out.append(fmtDouble(points.get(i).value()));
        }
        out.append(']').append(nl).append('}');
        return out.toString().getBytes(StandardCharsets.UTF_8);
    }

    public TsSeriesD decode(byte[] bytes) {
        String text = new String(bytes, StandardCharsets.UTF_8);
        long[] ts = extractLongArray(text, "timestamps");
        double[] vals = extractDoubleArray(text, "values");
        if (ts.length != vals.length) {
            throw TsCodecException.parse(
                    "timestamps (" + ts.length + ") and values (" + vals.length + ") length mismatch");
        }
        long scale;
        switch (style) {
            case EPOCH_NANOS -> scale = 1L;
            case EPOCH_MILLIS -> scale = 1_000_000L;
            default -> throw TsCodecException.unsupportedTimestampDecode();
        }
        TsSeriesD s = TsSeriesD.withCapacity(ts.length);
        for (int i = 0; i < ts.length; i++) {
            try {
                s.push(ts[i] * scale, vals[i]);
            } catch (TsException e) {
                throw TsCodecException.parse(e.getMessage());
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

    // Values are finite (push rejects NaN/inf). A whole number still prints
    // with a fractional part so the column stays unambiguously float.
    static String fmtDouble(double v) {
        if (v == Math.rint(v) && !Double.isInfinite(v) && Math.abs(v) < 1e15) {
            return String.format(java.util.Locale.ROOT, "%.1f", v);
        }
        return Double.toString(v);
    }

    private static String jsonString(String s) {
        StringBuilder out = new StringBuilder(s.length() + 2);
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> out.append(c);
            }
        }
        out.append('"');
        return out.toString();
    }

    private static long[] extractLongArray(String text, String key) {
        String body = arrayBody(text, key);
        List<Long> out = new ArrayList<>();
        for (String tok : body.split(",")) {
            String t = tok.trim();
            if (t.isEmpty()) continue;
            try {
                out.add(Long.parseLong(t));
            } catch (NumberFormatException e) {
                throw TsCodecException.parse("bad timestamp token: " + t);
            }
        }
        long[] arr = new long[out.size()];
        for (int i = 0; i < arr.length; i++) arr[i] = out.get(i);
        return arr;
    }

    private static double[] extractDoubleArray(String text, String key) {
        String body = arrayBody(text, key);
        List<Double> out = new ArrayList<>();
        for (String tok : body.split(",")) {
            String t = tok.trim();
            if (t.isEmpty()) continue;
            try {
                out.add(Double.parseDouble(t));
            } catch (NumberFormatException e) {
                throw TsCodecException.parse("bad value token: " + t);
            }
        }
        double[] arr = new double[out.size()];
        for (int i = 0; i < arr.length; i++) arr[i] = out.get(i);
        return arr;
    }

    private static String arrayBody(String text, String key) {
        String needle = "\"" + key + "\"";
        int kpos = text.indexOf(needle);
        if (kpos < 0) throw TsCodecException.parse("missing key " + key);
        int after = kpos + needle.length();
        int open = text.indexOf('[', after);
        if (open < 0) throw TsCodecException.parse("no array after " + key);
        int close = text.indexOf(']', open + 1);
        if (close < 0) throw TsCodecException.parse("unterminated array for " + key);
        return text.substring(open + 1, close);
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
        return String.format(java.util.Locale.ROOT,
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
