package com.submillisecond.recipes.tscsv;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsValue;

/**
 * Zero-dependency, hand-rolled CSV reader and writer for the typed
 * {@link TsDataFrame}. Each source column becomes a typed
 * {@code com.submillisecond.recipes.ts.TsColumn} over a row-index or designated
 * timestamp axis, with the element type inferred per column by the
 * narrowest-fit rule (see {@link TsTypeInfer}).
 *
 * <p>The parser is RFC-4180-ish: comma separator (configurable), double-quote
 * quoting with {@code ""} as the embedded-quote escape, and CRLF or LF line
 * endings. An empty cell is a gap - the row contributes no point to that
 * column, mirroring the frame's gap model; no null is ever pushed.
 */
public final class TsCsv {

    private TsCsv() {}

    /** Parse {@code text} as CSV into a {@link TsDataFrame}. */
    public static TsDataFrame readCsv(String text, TsCsvOptions opts) {
        List<List<String>> records = tokenize(text, opts.delim());
        if (records.isEmpty()) {
            return new TsDataFrame();
        }

        List<String> names;
        int firstData;
        if (opts.header()) {
            names = records.get(0);
            firstData = 1;
        } else {
            int width = records.get(0).size();
            names = new ArrayList<>(width);
            for (int i = 0; i < width; i++) {
                names.add("col" + i);
            }
            firstData = 0;
        }
        int width = names.size();

        int tsCol = -1;
        Optional<String> tsName = opts.ts();
        if (tsName.isPresent()) {
            tsCol = names.indexOf(tsName.get());
            if (tsCol < 0) {
                throw TsCsvException.unknownTsColumn(tsName.get());
            }
        }

        List<TsColumnBuilder> builders = new ArrayList<>(width);
        for (int i = 0; i < width; i++) {
            builders.add(new TsColumnBuilder());
        }

        for (int r = firstData; r < records.size(); r++) {
            List<String> record = records.get(r);
            if (record.size() != width) {
                throw TsCsvException.raggedRow(r, width, record.size());
            }

            long ts;
            if (tsCol >= 0) {
                String cell = record.get(tsCol);
                try {
                    ts = Long.parseLong(cell.trim());
                } catch (NumberFormatException e) {
                    throw TsCsvException.badTimestamp(r, cell);
                }
            } else {
                ts = r - firstData;
            }

            for (int c = 0; c < width; c++) {
                if (c == tsCol) {
                    continue;
                }
                String cell = record.get(c);
                if (cell.isEmpty()) {
                    continue; // gap: contribute no point
                }
                builders.get(c).add(ts, cell);
            }
        }

        TsDataFrame df = new TsDataFrame();
        for (int i = 0; i < width; i++) {
            if (i == tsCol) {
                continue;
            }
            df.pushColumn(names.get(i), builders.get(i).build());
        }
        return df;
    }

    /**
     * Split CSV text into records of fields. A record terminator is LF, CR, or
     * CRLF; a quoted field may contain the delimiter, a terminator, and
     * {@code ""}. A trailing terminator does not yield a spurious empty record.
     */
    static List<List<String>> tokenize(String text, char delim) {
        List<List<String>> records = new ArrayList<>();
        List<String> record = new ArrayList<>();
        StringBuilder field = new StringBuilder();
        boolean inQuotes = false;
        boolean fieldStarted = false;
        int row = 0;

        int i = 0;
        int len = text.length();
        while (i < len) {
            char c = text.charAt(i);
            if (inQuotes) {
                if (c == '"') {
                    if (i + 1 < len && text.charAt(i + 1) == '"') {
                        field.append('"');
                        i += 2;
                    } else {
                        inQuotes = false;
                        i++;
                    }
                } else {
                    field.append(c);
                    i++;
                }
                continue;
            }

            if (c == '"') {
                if (field.length() != 0) {
                    throw TsCsvException.badQuoting(row);
                }
                inQuotes = true;
                fieldStarted = true;
                i++;
            } else if (c == '\r') {
                if (i + 1 < len && text.charAt(i + 1) == '\n') {
                    i++;
                }
                record.add(field.toString());
                field.setLength(0);
                records.add(record);
                record = new ArrayList<>();
                fieldStarted = false;
                row++;
                i++;
            } else if (c == '\n') {
                record.add(field.toString());
                field.setLength(0);
                records.add(record);
                record = new ArrayList<>();
                fieldStarted = false;
                row++;
                i++;
            } else if (c == delim) {
                record.add(field.toString());
                field.setLength(0);
                fieldStarted = true;
                i++;
            } else {
                field.append(c);
                fieldStarted = true;
                i++;
            }
        }

        if (inQuotes) {
            throw TsCsvException.badQuoting(row);
        }

        if (fieldStarted || field.length() != 0 || !record.isEmpty()) {
            record.add(field.toString());
            records.add(record);
        }

        return records;
    }

    /**
     * Emit a {@link TsDataFrame} as CSV. The header is the column names; one row
     * per timestamp in the frame's row-aligned view; an empty cell where a
     * column has a gap at that ts. Cells containing the delimiter, a quote, or a
     * newline are double-quoted with {@code ""} escaping. Round-trips with
     * {@link #readCsv} for the inferred types.
     */
    public static String writeCsv(TsDataFrame df) {
        List<String> names = df.columnNames();
        StringBuilder out = new StringBuilder();

        for (int i = 0; i < names.size(); i++) {
            if (i > 0) {
                out.append(',');
            }
            writeCell(out, names.get(i));
        }
        out.append('\n');

        for (TsDataFrame.Row r : df.aligned()) {
            List<Optional<TsValue>> values = r.values();
            for (int i = 0; i < values.size(); i++) {
                if (i > 0) {
                    out.append(',');
                }
                Optional<TsValue> cell = values.get(i);
                if (cell.isPresent()) {
                    writeCell(out, valueToCell(cell.get()));
                }
            }
            out.append('\n');
        }

        return out.toString();
    }

    /** One cell of a row's value, rendered to the canonical text the reader will
     *  re-infer to the same type. */
    static String valueToCell(TsValue v) {
        return switch (v) {
            case TsValue.I64 x -> Long.toString(x.value());
            case TsValue.F64 x -> doubleToCell(x.value());
            case TsValue.Bool b -> Boolean.toString(b.value());
            case TsValue.Str s -> s.value();
            case TsValue.Bytes b -> "";
            case TsValue.Null n -> "";
            case TsValue.MapVal m -> "";
            case TsValue.Array a -> "";
        };
    }

    /** Render a double the way the Rust side's {@code f64::to_string} does for
     *  the common cases: an integral value keeps no fractional part only when it
     *  has none, otherwise the shortest round-trip form Java emits. */
    private static String doubleToCell(double v) {
        return Double.toString(v);
    }

    static void writeCell(StringBuilder out, String cell) {
        boolean needsQuote = cell.indexOf(',') >= 0
                || cell.indexOf('"') >= 0
                || cell.indexOf('\n') >= 0
                || cell.indexOf('\r') >= 0;
        if (needsQuote) {
            out.append('"');
            for (int i = 0; i < cell.length(); i++) {
                char ch = cell.charAt(i);
                if (ch == '"') {
                    out.append('"');
                }
                out.append(ch);
            }
            out.append('"');
        } else {
            out.append(cell);
        }
    }
}
