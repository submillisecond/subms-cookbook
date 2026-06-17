package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.OptionalLong;

/**
 * Decode a Flux annotated-CSV query response into a {@code TsCollection<Double>}.
 * Annotation rows ({@code #datatype} / {@code #group} / {@code #default}) are
 * ignored; the header names drive the parse. One series per (measurement,
 * tag-set); points land in time order. Byte for byte equivalent to the Rust
 * sibling's csv module.
 */
public final class FluxCsv {

    private static final List<String> RESERVED = List.of(
            "", "result", "table", "_start", "_stop", "_time", "_value", "_field");

    private FluxCsv() {}

    /** Influx series key: {@code measurement} when untagged, else
     * {@code measurement,k=v,...} with tags sorted by key. */
    static String seriesKey(String measurement, List<Map.Entry<String, String>> tags) {
        if (tags.isEmpty()) {
            return measurement;
        }
        List<Map.Entry<String, String>> sorted = new ArrayList<>(tags);
        sorted.sort(Map.Entry.comparingByKey());
        StringBuilder key = new StringBuilder(measurement);
        for (Map.Entry<String, String> t : sorted) {
            key.append(',').append(t.getKey()).append('=').append(t.getValue());
        }
        return key.toString();
    }

    static List<List<String>> parseRecords(String input) {
        List<List<String>> records = new ArrayList<>();
        StringBuilder field = new StringBuilder();
        List<String> record = new ArrayList<>();
        boolean inQuotes = false;
        boolean sawField = false;
        for (int i = 0; i < input.length(); i++) {
            char c = input.charAt(i);
            if (inQuotes) {
                if (c == '"') {
                    if (i + 1 < input.length() && input.charAt(i + 1) == '"') {
                        field.append('"');
                        i++;
                    } else {
                        inQuotes = false;
                    }
                } else {
                    field.append(c);
                }
            } else {
                switch (c) {
                    case '"' -> {
                        inQuotes = true;
                        sawField = true;
                    }
                    case ',' -> {
                        record.add(field.toString());
                        field.setLength(0);
                        sawField = true;
                    }
                    case '\r' -> {}
                    case '\n' -> {
                        if (sawField || field.length() > 0 || !record.isEmpty()) {
                            record.add(field.toString());
                            field.setLength(0);
                            records.add(record);
                            record = new ArrayList<>();
                        }
                        sawField = false;
                    }
                    default -> {
                        field.append(c);
                        sawField = true;
                    }
                }
            }
        }
        if (sawField || field.length() > 0 || !record.isEmpty()) {
            record.add(field.toString());
            records.add(record);
        }
        return records;
    }

    /** Decode an annotated-CSV Flux response. */
    public static TsCollection<Double> decodeResponse(String body) {
        List<List<String>> records = parseRecords(body);
        List<String> header = null;
        for (List<String> r : records) {
            if (r.contains("_time") && r.contains("_value")) {
                header = r;
                break;
            }
        }
        if (header == null) {
            throw TsInfluxException.csv("no header row with _time and _value");
        }
        int timeI = header.indexOf("_time");
        int valueI = header.indexOf("_value");
        int measI = header.indexOf("_measurement");
        List<int[]> tagCols = new ArrayList<>();
        List<String> tagNames = new ArrayList<>();
        for (int i = 0; i < header.size(); i++) {
            String n = header.get(i);
            if (!RESERVED.contains(n) && !n.equals("_measurement")) {
                tagCols.add(new int[] {i});
                tagNames.add(n);
            }
        }

        TsCollection<Double> coll = new TsCollection<>();
        Map<String, long[]> idByKey = new LinkedHashMap<>();
        Map<String, List<double[]>> ptsByKey = new LinkedHashMap<>();
        long[] nextId = {0};

        for (List<String> row : records) {
            if (row.size() < header.size() || row == header) {
                continue;
            }
            boolean annotation = false;
            for (String f : row) {
                if (f.startsWith("#")) {
                    annotation = true;
                    break;
                }
            }
            if (annotation) {
                continue;
            }
            String rawTime = row.get(timeI);
            if (rawTime.equals("_time") || rawTime.isEmpty()) {
                continue;
            }
            OptionalLong ts = Rfc3339.parseNanos(rawTime);
            if (ts.isEmpty()) {
                throw TsInfluxException.csv("unparseable _time");
            }
            double value;
            try {
                value = Double.parseDouble(row.get(valueI));
            } catch (NumberFormatException e) {
                throw TsInfluxException.csv("unparseable _value");
            }
            String measurement = measI >= 0 ? row.get(measI) : "";

            StringBuilder dedup = new StringBuilder(measurement);
            List<Map.Entry<String, String>> tags = new ArrayList<>();
            for (int ti = 0; ti < tagCols.size(); ti++) {
                String v = row.get(tagCols.get(ti)[0]);
                if (v.isEmpty()) {
                    continue;
                }
                dedup.append('\u0001').append(tagNames.get(ti)).append('=').append(v);
                tags.add(Map.entry(tagNames.get(ti), v));
            }
            String key = dedup.toString();

            if (!idByKey.containsKey(key)) {
                long id = nextId[0]++;
                TsSeriesMetadata meta = new TsSeriesMetadata(id, seriesKey(measurement, tags));
                for (Map.Entry<String, String> t : tags) {
                    meta = meta.withTag(t.getKey(), t.getValue());
                }
                long got = coll.register(meta);
                idByKey.put(key, new long[] {got});
                ptsByKey.put(key, new ArrayList<>());
            }
            ptsByKey.get(key).add(new double[] {(double) ts.getAsLong(), value});
        }

        for (Map.Entry<String, long[]> e : idByKey.entrySet()) {
            List<double[]> pts = ptsByKey.get(e.getKey());
            pts.sort((a, b) -> Double.compare(a[0], b[0]));
            long id = e.getValue()[0];
            for (double[] p : pts) {
                coll.push(id, (long) p[0], p[1]);
            }
        }
        return coll;
    }
}
