package com.submillisecond.recipes.health;

import java.util.Map;
import java.util.TreeMap;

/** Hand-rolled deterministic JSON helpers, byte-identical to the Rust + Python ports. */
final class Json {
    private Json() {}

    static void escape(StringBuilder sb, String s) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                case '\b' -> sb.append("\\b");
                case '\f' -> sb.append("\\f");
                default -> {
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
                }
            }
        }
        sb.append('"');
    }

    static void value(StringBuilder sb, Object v) {
        if (v instanceof Boolean b) {
            sb.append(b ? "true" : "false");
        } else if (v instanceof Integer || v instanceof Long) {
            sb.append(v.toString());
        } else {
            escape(sb, v == null ? "" : v.toString());
        }
    }

    /** {"k":v,...} with keys in sorted order. */
    static void map(StringBuilder sb, Map<String, Object> m) {
        sb.append('{');
        boolean first = true;
        for (Map.Entry<String, Object> e : new TreeMap<>(m).entrySet()) {
            if (!first) {
                sb.append(',');
            }
            first = false;
            escape(sb, e.getKey());
            sb.append(':');
            value(sb, e.getValue());
        }
        sb.append('}');
    }
}
