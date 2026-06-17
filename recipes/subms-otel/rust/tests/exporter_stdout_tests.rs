//! Smoke: stdout exporter helper builds a wired pair without panicking.

#![cfg(feature = "exporter-stdout")]

use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::trace::{Tracer, TracerProvider};
use opentelemetry_sdk::Resource;
use subms_otel::ExporterStdoutHelper;

fn fake_resource() -> Resource {
    Resource::builder_empty()
        .with_attributes([KeyValue::new("service.name", "subms-otel-test")])
        .build()
}

#[test]
fn helper_builds_and_records() {
    let (mp, tp) = ExporterStdoutHelper::build(fake_resource());
    let meter = mp.meter("smoke");
    let h = meter.f64_histogram("smoke.latency").build();
    h.record(0.0001, &[]);
    let tracer = tp.tracer("smoke");
    let mut span = tracer.span_builder("smoke.span").start(&tracer);
    use opentelemetry::trace::Span;
    span.end();
    let _ = mp.force_flush();
    let _ = tp.force_flush();
}
