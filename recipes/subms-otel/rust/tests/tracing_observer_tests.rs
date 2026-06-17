//! TracingObserver tests: per-record span emission + W3C parent context
//! inheritance.

#![cfg(feature = "tracing")]

mod common;

use std::sync::Arc;

use opentelemetry::Context;
use opentelemetry::global;
use opentelemetry::trace::{Span, TraceContextExt, Tracer, TracerProvider};
use opentelemetry_sdk::trace::SdkTracerProvider;

use subms::{ObservationCtx, SubMsObserver, SubMsStageKind};
use subms_otel::{TRACING_SPAN_NAME, TracingObserver};

use crate::common::InMemorySpanExporter;

fn build_tracer_provider() -> (InMemorySpanExporter, SdkTracerProvider) {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    (exporter, provider)
}

#[test]
fn on_record_emits_a_named_span() {
    let (exporter, provider) = build_tracer_provider();
    let tracer = provider.tracer("subms-otel-tracing-test");
    let observer = TracingObserver::new(tracer);

    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    observer.on_record(&ctx, 100);
    observer.on_record(&ctx, 200);

    let spans = exporter.get_finished_spans().unwrap();
    let record_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.name == TRACING_SPAN_NAME)
        .collect();
    assert_eq!(record_spans.len(), 2);
    let stage_attrs: Vec<&str> = record_spans[0]
        .attributes
        .iter()
        .map(|kv| kv.key.as_str())
        .collect();
    assert!(stage_attrs.contains(&"subms.stage"));
    assert!(stage_attrs.contains(&"subms.workload"));
    assert!(stage_attrs.contains(&"subms.stage.kind"));
}

#[test]
fn record_spans_inherit_active_parent_span() {
    let (exporter, provider) = build_tracer_provider();
    global::set_tracer_provider(provider.clone());
    let tracer = provider.tracer("subms-otel-tracing-test");

    let mut parent = tracer.span_builder("http-request").start(&tracer);
    let parent_span_id = parent.span_context().span_id();
    let cx = Context::current().with_remote_span_context(parent.span_context().clone());
    let guard = cx.attach();

    let observer = TracingObserver::new(tracer.clone());
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    observer.on_record(&ctx, 500);

    drop(guard);
    parent.end();

    let spans = exporter.get_finished_spans().unwrap();
    let child = spans
        .iter()
        .find(|s| s.name == TRACING_SPAN_NAME)
        .expect("record span should be present");

    assert_eq!(
        child.parent_span_id, parent_span_id,
        "the record span should inherit the active span as its parent"
    );
}

#[test]
fn observer_is_share_safe_via_arc() {
    let (_exporter, provider) = build_tracer_provider();
    let tracer = provider.tracer("share-test");
    let observer: Arc<dyn SubMsObserver> = Arc::new(TracingObserver::new(tracer));
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    observer.on_record(&ctx, 100);
}
