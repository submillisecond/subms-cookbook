//! Hand-rolled Prometheus text-format exporter. Implements the OTel-SDK
//! `PushMetricExporter` trait, formats every flushed instrument into a
//! Mutex-guarded `String` buffer, and exposes `scrape()` for HTTP handlers.
//!
//! Translation rules (per the OTel -> Prom spec):
//!
//! - dots in metric names + attribute keys become underscores
//! - histograms with unit `s` get `_seconds` appended
//! - counters get `_total` appended (if not already present)
//! - `+Inf` is the implicit final bucket on histograms

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry::Value;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

/// Push-style `MetricExporter` that materialises the latest OTel snapshot as
/// Prometheus text and stores it in a shared buffer. Consumers gather the
/// buffer via [`Self::scrape`] from any HTTP handler.
#[derive(Debug, Clone, Default)]
pub struct PrometheusTextExporter {
    buf: Arc<Mutex<String>>,
}

impl PrometheusTextExporter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of the latest scrape buffer. Empty until the first export
    /// completes.
    pub fn scrape(&self) -> String {
        self.buf.lock().expect("prom buffer poisoned").clone()
    }
}

impl PushMetricExporter for PrometheusTextExporter {
    async fn export(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let mut out = String::new();
        for sm in metrics.scope_metrics() {
            for m in sm.metrics() {
                render_instrument(&mut out, m.name(), m.unit(), m.description(), m.data());
            }
        }
        let mut guard = self.buf.lock().expect("prom buffer poisoned");
        *guard = out;
        Ok(())
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

fn render_instrument(
    out: &mut String,
    name: &str,
    unit: &str,
    description: &str,
    data: &AggregatedMetrics,
) {
    match data {
        AggregatedMetrics::F64(MetricData::Histogram(h)) => {
            let prom_name = histogram_name(name, unit);
            write_help_type(out, &prom_name, description, "histogram");
            for dp in h.data_points() {
                let labels = render_labels(dp.attributes());
                let mut cumulative: u64 = 0;
                let boundaries: Vec<f64> = dp.bounds().collect();
                let counts: Vec<u64> = dp.bucket_counts().collect();
                for (i, b) in boundaries.iter().enumerate() {
                    cumulative += counts.get(i).copied().unwrap_or(0);
                    write_bucket(out, &prom_name, &labels, &format_f64(*b), cumulative);
                }
                cumulative += counts.last().copied().unwrap_or(0);
                write_bucket(out, &prom_name, &labels, "+Inf", cumulative);
                let _ = writeln!(out, "{}_count{} {}", prom_name, labels, dp.count());
                let _ = writeln!(out, "{}_sum{} {}", prom_name, labels, format_f64(dp.sum()));
            }
        }
        AggregatedMetrics::U64(MetricData::Histogram(h)) => {
            let prom_name = histogram_name(name, unit);
            write_help_type(out, &prom_name, description, "histogram");
            for dp in h.data_points() {
                let labels = render_labels(dp.attributes());
                let mut cumulative: u64 = 0;
                let boundaries: Vec<f64> = dp.bounds().collect();
                let counts: Vec<u64> = dp.bucket_counts().collect();
                for (i, b) in boundaries.iter().enumerate() {
                    cumulative += counts.get(i).copied().unwrap_or(0);
                    write_bucket(out, &prom_name, &labels, &format_f64(*b), cumulative);
                }
                cumulative += counts.last().copied().unwrap_or(0);
                write_bucket(out, &prom_name, &labels, "+Inf", cumulative);
                let _ = writeln!(out, "{}_count{} {}", prom_name, labels, dp.count());
                let _ = writeln!(out, "{}_sum{} {}", prom_name, labels, dp.sum());
            }
        }
        AggregatedMetrics::U64(MetricData::Sum(s)) => {
            let prom_name = if s.is_monotonic() {
                counter_name(name)
            } else {
                sanitize_name(name)
            };
            let ty = if s.is_monotonic() { "counter" } else { "gauge" };
            write_help_type(out, &prom_name, description, ty);
            for dp in s.data_points() {
                let labels = render_labels(dp.attributes());
                let _ = writeln!(out, "{}{} {}", prom_name, labels, dp.value());
            }
        }
        AggregatedMetrics::I64(MetricData::Sum(s)) => {
            let prom_name = if s.is_monotonic() {
                counter_name(name)
            } else {
                sanitize_name(name)
            };
            let ty = if s.is_monotonic() { "counter" } else { "gauge" };
            write_help_type(out, &prom_name, description, ty);
            for dp in s.data_points() {
                let labels = render_labels(dp.attributes());
                let _ = writeln!(out, "{}{} {}", prom_name, labels, dp.value());
            }
        }
        AggregatedMetrics::F64(MetricData::Sum(s)) => {
            let prom_name = if s.is_monotonic() {
                counter_name(name)
            } else {
                sanitize_name(name)
            };
            let ty = if s.is_monotonic() { "counter" } else { "gauge" };
            write_help_type(out, &prom_name, description, ty);
            for dp in s.data_points() {
                let labels = render_labels(dp.attributes());
                let _ = writeln!(out, "{}{} {}", prom_name, labels, format_f64(dp.value()));
            }
        }
        AggregatedMetrics::U64(MetricData::Gauge(g)) => {
            let prom_name = sanitize_name(name);
            write_help_type(out, &prom_name, description, "gauge");
            for dp in g.data_points() {
                let labels = render_labels(dp.attributes());
                let _ = writeln!(out, "{}{} {}", prom_name, labels, dp.value());
            }
        }
        AggregatedMetrics::I64(MetricData::Gauge(g)) => {
            let prom_name = sanitize_name(name);
            write_help_type(out, &prom_name, description, "gauge");
            for dp in g.data_points() {
                let labels = render_labels(dp.attributes());
                let _ = writeln!(out, "{}{} {}", prom_name, labels, dp.value());
            }
        }
        AggregatedMetrics::F64(MetricData::Gauge(g)) => {
            let prom_name = sanitize_name(name);
            write_help_type(out, &prom_name, description, "gauge");
            for dp in g.data_points() {
                let labels = render_labels(dp.attributes());
                let _ = writeln!(out, "{}{} {}", prom_name, labels, format_f64(dp.value()));
            }
        }
        _ => {}
    }
}

fn write_help_type(out: &mut String, name: &str, description: &str, ty: &str) {
    if !description.is_empty() {
        let _ = writeln!(out, "# HELP {} {}", name, escape_help(description));
    }
    let _ = writeln!(out, "# TYPE {} {}", name, ty);
}

fn write_bucket(out: &mut String, name: &str, base_labels: &str, le: &str, count: u64) {
    let label = bucket_label(base_labels, le);
    let _ = writeln!(out, "{}_bucket{} {}", name, label, count);
}

fn bucket_label(base: &str, le: &str) -> String {
    if base.is_empty() {
        format!("{{le=\"{}\"}}", le)
    } else {
        let inner = &base[1..base.len() - 1];
        format!("{{{},le=\"{}\"}}", inner, le)
    }
}

fn render_labels<'a, I>(attrs: I) -> String
where
    I: Iterator<Item = &'a opentelemetry::KeyValue>,
{
    let mut sorted: BTreeMap<String, String> = BTreeMap::new();
    for kv in attrs {
        sorted.insert(
            sanitize_label_key(kv.key.as_str()),
            value_to_label(&kv.value),
        );
    }
    if sorted.is_empty() {
        return String::new();
    }
    let mut out = String::from("{");
    let mut first = true;
    for (k, v) in sorted {
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "{}=\"{}\"", k, escape_label_value(&v));
    }
    out.push('}');
    out
}

fn histogram_name(name: &str, unit: &str) -> String {
    let base = sanitize_name(name);
    if unit == "s" && !base.ends_with("_seconds") {
        format!("{}_seconds", base)
    } else {
        base
    }
}

fn counter_name(name: &str) -> String {
    let base = sanitize_name(name);
    if base.ends_with("_total") {
        base
    } else {
        format!("{}_total", base)
    }
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == ':' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn sanitize_label_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn value_to_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::I64(i) => i.to_string(),
        Value::F64(f) => format_f64(*f),
        other => format!("{:?}", other),
    }
}

fn escape_label_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}

fn escape_help(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}

fn format_f64(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v.is_sign_positive() {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        }
    } else if v == 0.0 {
        "0".to_string()
    } else {
        let s = format!("{}", v);
        s
    }
}

/// Thin builder around the in-tree text exporter. Holds a fresh
/// [`PrometheusTextExporter`] and the [`SdkMeterProvider`] wired against it.
pub struct PrometheusBuilder {
    exporter: PrometheusTextExporter,
}

impl Default for PrometheusBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PrometheusBuilder {
    pub fn new() -> Self {
        Self {
            exporter: PrometheusTextExporter::new(),
        }
    }

    pub fn with_exporter(exporter: PrometheusTextExporter) -> Self {
        Self { exporter }
    }

    pub fn exporter(&self) -> &PrometheusTextExporter {
        &self.exporter
    }

    pub fn build(self) -> Option<(SdkMeterProvider, PrometheusTextExporter)> {
        let reader = PeriodicReader::builder(self.exporter.clone())
            .with_interval(Duration::from_secs(60))
            .build();
        let provider = SdkMeterProvider::builder().with_reader(reader).build();
        Some((provider, self.exporter))
    }
}

/// Multi-modal helper: returns wired `MeterProvider` + the in-tree text
/// exporter. Prometheus is metrics-only, so no `TracerProvider` is returned -
/// autoconfig pairs this with a no-op tracer provider.
pub struct ExporterPrometheusHelper;

impl ExporterPrometheusHelper {
    pub fn build(resource: Resource) -> Option<(SdkMeterProvider, PrometheusTextExporter)> {
        let exporter = PrometheusTextExporter::new();
        let reader = PeriodicReader::builder(exporter.clone())
            .with_interval(Duration::from_secs(60))
            .build();
        let provider = SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource)
            .build();
        Some((provider, exporter))
    }
}

#[cfg(test)]
#[path = "exporter_prometheus_tests.rs"]
mod exporter_prometheus_tests;
