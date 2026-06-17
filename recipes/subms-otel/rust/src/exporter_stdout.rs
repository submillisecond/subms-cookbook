//! Stdout exporter helper. Hand-rolled `PushMetricExporter` + `SpanExporter`
//! that emit one JSON line per metric / span to stdout. Useful as the
//! `autoconfig` fallback when no `OTEL_EXPORTER_OTLP_ENDPOINT` is set but a
//! developer still wants to see emissions during local debugging.

use std::fmt::Write as _;
use std::io::Write;
use std::time::Duration;

use opentelemetry::Value;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter};

pub struct ExporterStdoutHelper;

impl ExporterStdoutHelper {
    pub fn build(resource: Resource) -> (SdkMeterProvider, SdkTracerProvider) {
        let reader = PeriodicReader::builder(StdoutMetricExporter)
            .with_interval(Duration::from_secs(60))
            .build();
        let meter_provider = SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource.clone())
            .build();

        let tracer_provider = SdkTracerProvider::builder()
            .with_simple_exporter(StdoutSpanExporter)
            .with_resource(resource)
            .build();
        (meter_provider, tracer_provider)
    }
}

#[derive(Debug)]
struct StdoutMetricExporter;

impl PushMetricExporter for StdoutMetricExporter {
    async fn export(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let mut out = std::io::stdout().lock();
        for sm in metrics.scope_metrics() {
            for m in sm.metrics() {
                let mut line = String::new();
                let _ = write!(
                    &mut line,
                    "{{\"kind\":\"metric\",\"name\":\"{}\",\"unit\":\"{}\",\"points\":[",
                    json_escape(m.name()),
                    json_escape(m.unit())
                );
                let mut first = true;
                match m.data() {
                    AggregatedMetrics::F64(MetricData::Histogram(h)) => {
                        for dp in h.data_points() {
                            push_sep(&mut line, &mut first);
                            let _ = write!(
                                &mut line,
                                "{{\"count\":{},\"sum\":{}}}",
                                dp.count(),
                                dp.sum()
                            );
                        }
                    }
                    AggregatedMetrics::U64(MetricData::Sum(s)) => {
                        for dp in s.data_points() {
                            push_sep(&mut line, &mut first);
                            let _ = write!(&mut line, "{{\"value\":{}}}", dp.value());
                        }
                    }
                    AggregatedMetrics::U64(MetricData::Gauge(g)) => {
                        for dp in g.data_points() {
                            push_sep(&mut line, &mut first);
                            let _ = write!(&mut line, "{{\"value\":{}}}", dp.value());
                        }
                    }
                    _ => {}
                }
                line.push_str("]}");
                let _ = writeln!(out, "{}", line);
            }
        }
        let _ = out.flush();
        Ok(())
    }

    fn force_flush(&self) -> OTelSdkResult {
        let _ = std::io::stdout().flush();
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        let _ = std::io::stdout().flush();
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

#[derive(Debug)]
struct StdoutSpanExporter;

impl SpanExporter for StdoutSpanExporter {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        let mut out = std::io::stdout().lock();
        for span in batch {
            let mut line = String::new();
            let _ = write!(
                &mut line,
                "{{\"kind\":\"span\",\"name\":\"{}\",\"trace_id\":\"{}\",\"span_id\":\"{}\",\"attrs\":{{",
                json_escape(&span.name),
                span.span_context.trace_id(),
                span.span_context.span_id(),
            );
            let mut first = true;
            for kv in span.attributes.iter() {
                push_sep(&mut line, &mut first);
                let _ = write!(
                    &mut line,
                    "\"{}\":\"{}\"",
                    json_escape(kv.key.as_str()),
                    json_escape(&value_to_string(&kv.value))
                );
            }
            line.push_str("}}");
            let _ = writeln!(out, "{}", line);
        }
        let _ = out.flush();
        Ok(())
    }
}

fn push_sep(buf: &mut String, first: &mut bool) {
    if *first {
        *first = false;
    } else {
        buf.push(',');
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::I64(i) => i.to_string(),
        Value::F64(f) => f.to_string(),
        other => format!("{:?}", other),
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(&mut out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
