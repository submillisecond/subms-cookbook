package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * InfluxDB line protocol encoder, JDK-only. Shape per point:
 * {@code measurement[,tagkey=tagval...] v=value timestamp}. Tags are emitted in
 * key order (a {@code TsTags} is a {@code TreeMap}). The field key is fixed to
 * {@code v}; timestamps are nanoseconds. Byte for byte equivalent to the Rust
 * sibling's line module.
 */
public final class LineProtocol {

    private LineProtocol() {}

    static String escapeMeasurement(String s) {
        return escape(s, ", ");
    }

    static String escapeTag(String s) {
        return escape(s, ",= ");
    }

    private static String escape(String s, String specials) {
        StringBuilder out = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (specials.indexOf(c) >= 0) {
                out.append('\\');
            }
            out.append(c);
        }
        return out.toString();
    }

    private static String fmtValue(double v) {
        if (v == Math.rint(v) && Double.isFinite(v) && Math.abs(v) < 1e16) {
            return String.format("%.1f", v);
        }
        return Double.toString(v);
    }

    /** Encode one point line (no trailing newline) into {@code out}. */
    public static void encodeLine(
            String measurement,
            List<Map.Entry<String, String>> tags,
            double value,
            long ts,
            StringBuilder out) {
        out.append(escapeMeasurement(measurement));
        for (Map.Entry<String, String> t : tags) {
            out.append(',').append(escapeTag(t.getKey())).append('=').append(escapeTag(t.getValue()));
        }
        out.append(" v=").append(fmtValue(value)).append(' ').append(ts);
    }

    /** Encode a double series, measurement defaulting to the metadata name. */
    public static String encodeSeries(TsSeriesD series, String measurement) {
        return encode(series.toList(), series.metadata(), measurement);
    }

    /** Encode a generic {@code TsSeries<Double>} (the collection element shape). */
    public static String encodeSeries(TsSeries<Double> series, String measurement) {
        List<TsPoint<Double>> pts = new ArrayList<>();
        for (TsPoint<Double> p : series) {
            pts.add(p);
        }
        return encode(pts, series.metadata(), measurement);
    }

    private static String encode(
            List<TsPoint<Double>> points, Optional<TsSeriesMetadata> meta, String measurement) {
        String name = !measurement.isEmpty()
                ? measurement
                : meta.map(TsSeriesMetadata::name).orElse("");
        List<Map.Entry<String, String>> tags = meta
                .map(m -> new ArrayList<Map.Entry<String, String>>(m.tags().entrySet()))
                .orElseGet(ArrayList::new);

        StringBuilder out = new StringBuilder();
        for (TsPoint<Double> p : points) {
            if (out.length() > 0) {
                out.append('\n');
            }
            encodeLine(name, tags, p.value(), p.ts(), out);
        }
        return out.toString();
    }

    /** Encode every series in a collection, one measurement per series. */
    public static String encodeCollection(TsCollection<Double> coll) {
        StringBuilder out = new StringBuilder();
        for (TsSeries<Double> s : coll.series()) {
            String body = encodeSeries(s, "");
            if (body.isEmpty()) {
                continue;
            }
            if (out.length() > 0) {
                out.append('\n');
            }
            out.append(body);
        }
        return out.toString();
    }
}
