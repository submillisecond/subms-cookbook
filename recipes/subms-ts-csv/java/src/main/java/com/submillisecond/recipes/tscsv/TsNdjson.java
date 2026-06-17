package com.submillisecond.recipes.tscsv;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.ts.TsDataFrame;

/**
 * Hand-rolled NDJSON ingest: one flat JSON object per line. The reader takes
 * the union of object keys (in first-seen order) as the column set; a key
 * absent on a line is a gap for that column at that row. A quoted value is
 * forced to {@code Str}; an unquoted token is inferred like a CSV cell.
 *
 * <p>This is deliberately a FLAT-object parser. A value that is itself an object
 * or an array is rejected ({@link TsCsvException.Kind#BAD_JSON}) rather than
 * flattened into synthetic columns - nested-JSON-to-columns is a non-claim.
 */
public final class TsNdjson {

    private TsNdjson() {}

    /** A scanned field value plus whether it arrived quoted (forces {@code Str}). */
    private record Scanned(String raw, boolean quoted) {}

    /** Parse {@code text} as NDJSON into a {@link TsDataFrame}. Blank lines are
     *  skipped. The ts axis is {@code opts.tsColumn} (its value parsed as a
     *  {@code long}) or the row index. */
    public static TsDataFrame readNdjson(String text, TsCsvOptions opts) {
        List<String> names = new ArrayList<>();
        List<TsColumnBuilder> builders = new ArrayList<>();

        String tsName = opts.ts().orElse(null);
        long row = 0;
        String[] lines = text.split("\n", -1);
        for (int lineNo = 0; lineNo < lines.length; lineNo++) {
            String line = lines[lineNo];
            if (line.isBlank()) {
                continue;
            }
            List<Field> fields = parseObject(line, lineNo);

            long ts;
            if (tsName != null) {
                Field tf = null;
                for (Field f : fields) {
                    if (f.key.equals(tsName)) {
                        tf = f;
                        break;
                    }
                }
                if (tf == null) {
                    throw TsCsvException.badTimestamp(lineNo, "missing key " + tsName);
                }
                try {
                    ts = Long.parseLong(tf.value.raw().trim());
                } catch (NumberFormatException e) {
                    throw TsCsvException.badTimestamp(lineNo, tf.value.raw());
                }
            } else {
                ts = row;
            }

            for (Field f : fields) {
                if (f.key.equals(tsName)) {
                    continue;
                }
                // a JSON null is a gap, same as a CSV empty cell.
                if (!f.value.quoted() && f.value.raw().equals("null")) {
                    continue;
                }
                int idx = names.indexOf(f.key);
                if (idx < 0) {
                    names.add(f.key);
                    builders.add(new TsColumnBuilder());
                    idx = names.size() - 1;
                }
                TsColumnBuilder b = builders.get(idx);
                if (f.value.quoted()) {
                    b.forceStr();
                }
                b.add(ts, f.value.raw());
            }
            row++;
        }

        TsDataFrame df = new TsDataFrame();
        for (int i = 0; i < names.size(); i++) {
            df.pushColumn(names.get(i), builders.get(i).build());
        }
        return df;
    }

    private record Field(String key, Scanned value) {}

    private static List<Field> parseObject(String line, int lineNo) {
        char[] b = line.toCharArray();
        int[] i = {0};

        skipWs(b, i);
        if (peek(b, i) != '{') {
            throw TsCsvException.badJson(lineNo, "expected object");
        }
        i[0]++;
        skipWs(b, i);

        List<Field> out = new ArrayList<>();
        if (peek(b, i) == '}') {
            i[0]++;
            skipWs(b, i);
            if (i[0] != b.length) {
                throw TsCsvException.badJson(lineNo, "trailing content after object");
            }
            return out;
        }

        while (true) {
            skipWs(b, i);
            if (peek(b, i) != '"') {
                throw TsCsvException.badJson(lineNo, "expected string key");
            }
            String key = parseString(b, i, lineNo);
            skipWs(b, i);
            if (peek(b, i) != ':') {
                throw TsCsvException.badJson(lineNo, "expected colon");
            }
            i[0]++;
            skipWs(b, i);
            Scanned value = parseValue(b, i, lineNo);
            out.add(new Field(key, value));
            skipWs(b, i);
            char c = peek(b, i);
            if (c == ',') {
                i[0]++;
            } else if (c == '}') {
                i[0]++;
                break;
            } else {
                throw TsCsvException.badJson(lineNo, "expected comma or close brace");
            }
        }

        skipWs(b, i);
        if (i[0] != b.length) {
            throw TsCsvException.badJson(lineNo, "trailing content after object");
        }
        return out;
    }

    private static char peek(char[] b, int[] i) {
        return i[0] < b.length ? b[i[0]] : '\0';
    }

    private static void skipWs(char[] b, int[] i) {
        while (i[0] < b.length && Character.isWhitespace(b[i[0]])) {
            i[0]++;
        }
    }

    private static String parseString(char[] b, int[] i, int lineNo) {
        if (peek(b, i) != '"') {
            throw TsCsvException.badJson(lineNo, "bad string");
        }
        i[0]++;
        StringBuilder s = new StringBuilder();
        while (i[0] < b.length) {
            char c = b[i[0]++];
            if (c == '"') {
                return s.toString();
            }
            if (c == '\\') {
                if (i[0] >= b.length) {
                    throw TsCsvException.badJson(lineNo, "bad escape");
                }
                char esc = b[i[0]++];
                switch (esc) {
                    case '"' -> s.append('"');
                    case '\\' -> s.append('\\');
                    case '/' -> s.append('/');
                    case 'n' -> s.append('\n');
                    case 'r' -> s.append('\r');
                    case 't' -> s.append('\t');
                    case 'b' -> s.append('\b');
                    case 'f' -> s.append('\f');
                    case 'u' -> {
                        if (i[0] + 4 > b.length) {
                            throw TsCsvException.badJson(lineNo, "bad unicode escape");
                        }
                        int code = 0;
                        for (int k = 0; k < 4; k++) {
                            int d = Character.digit(b[i[0]++], 16);
                            if (d < 0) {
                                throw TsCsvException.badJson(lineNo, "bad unicode escape");
                            }
                            code = code * 16 + d;
                        }
                        s.append((char) code);
                    }
                    default -> throw TsCsvException.badJson(lineNo, "bad escape");
                }
            } else {
                s.append(c);
            }
        }
        throw TsCsvException.badJson(lineNo, "unterminated string");
    }

    private static Scanned parseValue(char[] b, int[] i, int lineNo) {
        char c = peek(b, i);
        if (c == '"') {
            return new Scanned(parseString(b, i, lineNo), true);
        }
        if (c == '{' || c == '[') {
            throw TsCsvException.badJson(lineNo, "nested object/array not supported");
        }
        if (c == '\0') {
            throw TsCsvException.badJson(lineNo, "missing value");
        }
        int start = i[0];
        while (i[0] < b.length) {
            char ch = b[i[0]];
            if (ch == ',' || ch == '}' || ch == ']' || Character.isWhitespace(ch)) {
                break;
            }
            i[0]++;
        }
        String token = new String(b, start, i[0] - start);
        if (token.isEmpty()) {
            throw TsCsvException.badJson(lineNo, "empty value");
        }
        return new Scanned(token, false);
    }
}
