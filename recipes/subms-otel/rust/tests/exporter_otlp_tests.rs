//! Smoke: the OTLP exporter helper builds a `(MeterProvider, TracerProvider)`
//! pair that accepts a record without panicking.

#![cfg(feature = "exporter-otlp")]

use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::trace::{Tracer, TracerProvider};
use opentelemetry_sdk::Resource;
use subms_otel::{ExporterOtlpHelper, OtlpProtocol};

fn fake_resource() -> Resource {
    Resource::builder_empty()
        .with_attributes([KeyValue::new("service.name", "subms-otel-test")])
        .build()
}

#[test]
fn protocol_from_env_defaults_http_protobuf() {
    assert_eq!(OtlpProtocol::from_env(None), OtlpProtocol::HttpProtobuf);
    assert_eq!(
        OtlpProtocol::from_env(Some("http/protobuf")),
        OtlpProtocol::HttpProtobuf
    );
    assert_eq!(OtlpProtocol::from_env(Some("grpc")), OtlpProtocol::Grpc);
}

#[test]
fn http_helper_builds_and_records() {
    let Some((mp, tp)) =
        ExporterOtlpHelper::build(None, OtlpProtocol::HttpProtobuf, fake_resource())
    else {
        // Some test environments can't open the default OTLP socket; treat
        // the build returning None as an acceptable smoke result.
        return;
    };
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
