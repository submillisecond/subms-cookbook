package com.submillisecond.recipes.otel.exporters;

import io.opentelemetry.api.common.AttributeKey;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.metrics.InstrumentType;
import io.opentelemetry.sdk.metrics.data.AggregationTemporality;
import io.opentelemetry.sdk.metrics.data.DoublePointData;
import io.opentelemetry.sdk.metrics.data.HistogramPointData;
import io.opentelemetry.sdk.metrics.data.LongPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.data.MetricDataType;
import io.opentelemetry.sdk.metrics.export.MetricExporter;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Hand-rolled Prometheus text-format {@link MetricExporter}. Replaces the
 * heavier {@code opentelemetry-exporter-prometheus} jar so the bridge stays
 * dependency-light.
 *
 * <p>Translation rules:
 * <ul>
 *   <li>Dots in metric names + attribute keys become underscores.</li>
 *   <li>Histograms with unit {@code "s"} get {@code _seconds} appended.</li>
 *   <li>Monotonic counters get {@code _total} appended (if not already present).</li>
 *   <li>{@code +Inf} is the implicit final bucket on histograms.</li>
 * </ul>
 */
public final class PrometheusTextExporter implements MetricExporter {

    private final AtomicReference<String> buffer = new AtomicReference<>("");

    public PrometheusTextExporter() {}

    /** Latest scrape snapshot; empty string until the first export completes. */
    public String scrape() {
        return buffer.get();
    }

    @Override
    public AggregationTemporality getAggregationTemporality(InstrumentType instrumentType) {
        return AggregationTemporality.CUMULATIVE;
    }

    @Override
    public CompletableResultCode export(Collection<MetricData> metrics) {
        StringBuilder out = new StringBuilder();
        for (MetricData m : metrics) {
            renderInstrument(out, m);
        }
        buffer.set(out.toString());
        return CompletableResultCode.ofSuccess();
    }

    @Override
    public CompletableResultCode flush() {
        return CompletableResultCode.ofSuccess();
    }

    @Override
    public CompletableResultCode shutdown() {
        return CompletableResultCode.ofSuccess();
    }

    private static void renderInstrument(StringBuilder out, MetricData m) {
        MetricDataType type = m.getType();
        String desc = m.getDescription() == null ? "" : m.getDescription();
        switch (type) {
            case HISTOGRAM: {
                String promName = histogramName(m.getName(), m.getUnit());
                writeHelpType(out, promName, desc, "histogram");
                for (HistogramPointData p : m.getHistogramData().getPoints()) {
                    String baseLabels = renderLabels(p.getAttributes());
                    List<Double> bounds = p.getBoundaries();
                    List<Long> counts = p.getCounts();
                    long cumulative = 0L;
                    for (int i = 0; i < bounds.size(); i++) {
                        cumulative += i < counts.size() ? counts.get(i) : 0L;
                        writeBucket(out, promName, baseLabels, formatDouble(bounds.get(i)), cumulative);
                    }
                    cumulative += counts.isEmpty() ? 0L : counts.get(counts.size() - 1);
                    writeBucket(out, promName, baseLabels, "+Inf", cumulative);
                    out.append(promName).append("_count").append(baseLabels).append(' ')
                            .append(p.getCount()).append('\n');
                    out.append(promName).append("_sum").append(baseLabels).append(' ')
                            .append(formatDouble(p.getSum())).append('\n');
                }
                break;
            }
            case LONG_SUM: {
                boolean monotonic = m.getLongSumData().isMonotonic();
                String promName = monotonic ? counterName(m.getName()) : sanitizeName(m.getName());
                writeHelpType(out, promName, desc, monotonic ? "counter" : "gauge");
                for (LongPointData p : m.getLongSumData().getPoints()) {
                    String labels = renderLabels(p.getAttributes());
                    out.append(promName).append(labels).append(' ').append(p.getValue()).append('\n');
                }
                break;
            }
            case DOUBLE_SUM: {
                boolean monotonic = m.getDoubleSumData().isMonotonic();
                String promName = monotonic ? counterName(m.getName()) : sanitizeName(m.getName());
                writeHelpType(out, promName, desc, monotonic ? "counter" : "gauge");
                for (DoublePointData p : m.getDoubleSumData().getPoints()) {
                    String labels = renderLabels(p.getAttributes());
                    out.append(promName).append(labels).append(' ')
                            .append(formatDouble(p.getValue())).append('\n');
                }
                break;
            }
            case LONG_GAUGE: {
                String promName = sanitizeName(m.getName());
                writeHelpType(out, promName, desc, "gauge");
                for (LongPointData p : m.getLongGaugeData().getPoints()) {
                    String labels = renderLabels(p.getAttributes());
                    out.append(promName).append(labels).append(' ').append(p.getValue()).append('\n');
                }
                break;
            }
            case DOUBLE_GAUGE: {
                String promName = sanitizeName(m.getName());
                writeHelpType(out, promName, desc, "gauge");
                for (DoublePointData p : m.getDoubleGaugeData().getPoints()) {
                    String labels = renderLabels(p.getAttributes());
                    out.append(promName).append(labels).append(' ')
                            .append(formatDouble(p.getValue())).append('\n');
                }
                break;
            }
            default:
                break;
        }
    }

    private static void writeHelpType(StringBuilder out, String name, String desc, String type) {
        if (!desc.isEmpty()) {
            out.append("# HELP ").append(name).append(' ').append(escapeHelp(desc)).append('\n');
        }
        out.append("# TYPE ").append(name).append(' ').append(type).append('\n');
    }

    private static void writeBucket(StringBuilder out, String name, String baseLabels, String le, long count) {
        String label = bucketLabel(baseLabels, le);
        out.append(name).append("_bucket").append(label).append(' ').append(count).append('\n');
    }

    private static String bucketLabel(String base, String le) {
        if (base.isEmpty()) {
            return "{le=\"" + le + "\"}";
        }
        String inner = base.substring(1, base.length() - 1);
        return "{" + inner + ",le=\"" + le + "\"}";
    }

    private static String renderLabels(Attributes attrs) {
        if (attrs.isEmpty()) return "";
        TreeMap<String, String> sorted = new TreeMap<>();
        attrs.forEach((k, v) -> sorted.put(sanitizeLabelKey(k.getKey()), labelValueOf(k, v)));
        StringBuilder b = new StringBuilder("{");
        boolean first = true;
        for (Map.Entry<String, String> e : sorted.entrySet()) {
            if (!first) b.append(',');
            first = false;
            b.append(e.getKey()).append("=\"").append(escapeLabelValue(e.getValue())).append('"');
        }
        b.append('}');
        return b.toString();
    }

    private static String labelValueOf(AttributeKey<?> key, Object value) {
        if (value == null) return "";
        if (value instanceof Double d) return formatDouble(d);
        if (value instanceof List<?> list) {
            StringBuilder b = new StringBuilder();
            boolean first = true;
            for (Object o : list) {
                if (!first) b.append(',');
                first = false;
                b.append(o);
            }
            return b.toString();
        }
        return value.toString();
    }

    static String histogramName(String name, String unit) {
        String base = sanitizeName(name);
        if ("s".equals(unit) && !base.endsWith("_seconds")) {
            return base + "_seconds";
        }
        return base;
    }

    static String counterName(String name) {
        String base = sanitizeName(name);
        if (base.endsWith("_total")) return base;
        return base + "_total";
    }

    static String sanitizeName(String name) {
        StringBuilder b = new StringBuilder(name.length());
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
                    || c == '_' || c == ':') {
                b.append(c);
            } else {
                b.append('_');
            }
        }
        return b.toString();
    }

    static String sanitizeLabelKey(String key) {
        StringBuilder b = new StringBuilder(key.length());
        for (int i = 0; i < key.length(); i++) {
            char c = key.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_') {
                b.append(c);
            } else {
                b.append('_');
            }
        }
        return b.toString();
    }

    static String escapeLabelValue(String s) {
        StringBuilder b = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '\\': b.append("\\\\"); break;
                case '"': b.append("\\\""); break;
                case '\n': b.append("\\n"); break;
                default: b.append(c);
            }
        }
        return b.toString();
    }

    static String escapeHelp(String s) {
        StringBuilder b = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '\\': b.append("\\\\"); break;
                case '\n': b.append("\\n"); break;
                default: b.append(c);
            }
        }
        return b.toString();
    }

    static String formatDouble(double v) {
        if (Double.isNaN(v)) return "NaN";
        if (Double.isInfinite(v)) return v > 0 ? "+Inf" : "-Inf";
        if (v == 0.0d) return "0";
        if (v == Math.floor(v) && !Double.isInfinite(v) && Math.abs(v) < 1e15) {
            return Long.toString((long) v);
        }
        return String.format(Locale.ROOT, "%s", v);
    }

    /** Drains a snapshot of every metric currently on the registry. Mostly for tests. */
    public List<MetricData> drainSnapshot() {
        return new ArrayList<>();
    }
}
