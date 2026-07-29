//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the health/event bridge forwards, the observer emits one op per record, the
//! reservoir keeps the slow tail, tracing emits a span per record, and each
//! exporter helper wires up. Feature-gated the same way as the sample.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use subms_events::{Event, EventBridge, EventDispatcher, EventLevel, listener};

use crate::test_common::{InMemoryMetricExporter, sum_counter};
use crate::{OtelEventBridge, StateTransitionRecorder};

fn metric_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

#[test]
fn base_events_fan_out_and_transitions_count() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let tap = Arc::clone(&seen);

    let mut bus = EventDispatcher::sync();
    let bridge = OtelEventBridge::new();
    assert_eq!(bridge.name(), "otel");
    bus.add_bridge(Arc::new(bridge));
    bus.add_listener(listener(move |e: &Event| {
        tap.lock().unwrap().push(e.topic.clone())
    }));

    bus.emit(Event::transition(
        "gateway.health",
        EventLevel::Warn,
        "gateway",
        "UP",
        "DEGRADED",
    ));
    bus.emit(Event::builder("order.rejected").build());
    bus.emit(Event::transition(
        "gateway.health",
        EventLevel::Info,
        "gateway",
        "DEGRADED",
        "UP",
    ));

    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["gateway.health", "order.rejected", "gateway.health"],
        "every event reaches every listener the OTel bridge sits alongside"
    );

    let (exporter, provider) = metric_provider();
    let health = StateTransitionRecorder::with_meter(
        &provider.meter("test"),
        "gateway.health.transitions",
        "flips",
    );
    health.record(
        "gateway",
        "UP",
        "DEGRADED",
        &[("reason", "spread".to_string())],
    );
    health.record("gateway", "DEGRADED", "UP", &[]);
    provider.force_flush().expect("flush");

    let snaps = exporter.get_finished_metrics().expect("metrics");
    assert_eq!(
        sum_counter(&snaps, "gateway.health.transitions"),
        2,
        "both transitions land on the counter"
    );
}

#[cfg(feature = "observer")]
#[test]
fn observer_emits_one_op_per_record() {
    use crate::{ObservationCtx, OtelObserver, SubMsObserver, SubMsStageKind};

    let (exporter, provider) = metric_provider();
    let observer = OtelObserver::new(provider.meter("test"));
    let ctx = ObservationCtx {
        workload: "gateway-submit",
        lang: "rust",
        stage: "submit",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in [120u64, 180, 240, 160, 900] {
        observer.on_record(&ctx, ns);
    }
    provider.force_flush().expect("flush");

    let snaps = exporter.get_finished_metrics().expect("metrics");
    assert_eq!(
        sum_counter(&snaps, "subms.bench.ops_total"),
        5,
        "one op per record"
    );
}

#[cfg(feature = "exemplars")]
#[test]
fn reservoir_keeps_slowest_in_bucket() {
    use crate::{ExemplarReservoir, ObservationCtx, SubMsStageKind};

    let reservoir = ExemplarReservoir::with_capacity(3);
    let ctx = ObservationCtx {
        workload: "gateway-submit",
        lang: "rust",
        stage: "submit",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in [600u64, 950, 700, 900, 800] {
        reservoir.offer(&ctx, ns);
    }
    let mut kept: Vec<u64> = reservoir.snapshot().iter().map(|e| e.ns).collect();
    kept.sort_unstable();
    assert_eq!(
        kept,
        vec![800, 900, 950],
        "slowest three in the shared bucket"
    );
}

#[cfg(feature = "tracing")]
#[test]
fn tracing_emits_one_span_per_record() {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::SdkTracerProvider;

    use crate::test_common::InMemorySpanExporter;
    use crate::{
        ObservationCtx, SubMsObserver, SubMsStageKind, TRACING_SPAN_NAME, TracingObserver,
    };

    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let observer = TracingObserver::new(provider.tracer("test"));
    let ctx = ObservationCtx {
        workload: "gateway-submit",
        lang: "rust",
        stage: "submit",
        stage_kind: SubMsStageKind::HotPath,
    };
    observer.on_record(&ctx, 240);
    observer.on_record(&ctx, 310);

    let spans = exporter.get_finished_spans().expect("spans");
    assert_eq!(spans.len(), 2, "one span per record");
    assert!(spans.iter().all(|s| s.name == TRACING_SPAN_NAME));
}

#[cfg(feature = "autoconfig")]
#[test]
fn autoconfig_registry_round_trips() {
    use crate::{
        OtelObserver, clear_registered_observers, register_observer, registered_observers,
    };

    let (_exporter, provider) = metric_provider();
    clear_registered_observers();
    register_observer(
        "gateway-otel",
        Arc::new(OtelObserver::new(provider.meter("test"))),
    );
    assert_eq!(registered_observers().len(), 1);
    clear_registered_observers();
    assert_eq!(registered_observers().len(), 0);
}

#[cfg(feature = "exporter-otlp")]
#[test]
fn otlp_builder_wires_endpoint() {
    use crate::{OtlpBuilder, OtlpProtocol};

    assert_eq!(OtlpProtocol::from_env(Some("grpc")), OtlpProtocol::Grpc);
    assert_eq!(OtlpProtocol::from_env(None), OtlpProtocol::HttpProtobuf);
    let provider = OtlpBuilder::new()
        .with_endpoint("http://localhost:4318")
        .build();
    assert!(
        provider.is_some(),
        "OTLP exporter builds against the endpoint"
    );
}

#[cfg(feature = "exporter-prometheus")]
#[test]
fn prometheus_scrape_carries_the_histogram() {
    use crate::{PrometheusBuilder, SubMsBenchSummary, SubMsStageSummary, export_summary};
    use std::collections::BTreeMap;

    let (provider, exporter) = PrometheusBuilder::new().build().expect("provider");
    let meter = provider.meter("test");

    let summary = SubMsBenchSummary {
        workload: "gateway-submit".to_string(),
        lang: "rust".to_string(),
        timestamp: "2026-07-28T00:00:00Z".to_string(),
        cpu_core: None,
        cpu_affinity: None,
        inputs: BTreeMap::new(),
        meta: BTreeMap::new(),
        stages: vec![SubMsStageSummary {
            name: "submit".to_string(),
            count: 5,
            p50_ns: 160,
            p99_ns: 900,
            p999_ns: 2100,
            max_ns: 2100,
            mean_ns: 320,
            stddev_ns: 40,
            cdf_buckets_ns: vec![0; 64],
            jitter_score: 0.0,
            samples_ns: Some(vec![120, 180, 240, 160, 900]),
        }],
    };
    export_summary(&summary, &meter);
    provider.force_flush().expect("flush");

    let text = exporter.scrape();
    assert!(
        text.contains("subms_latency_seconds"),
        "histogram lands in Prometheus text"
    );
    assert!(
        text.contains("subms_stage=\"submit\""),
        "stage becomes a label"
    );
}

#[cfg(feature = "exporter-stdout")]
#[test]
fn stdout_helper_builds_providers() {
    use crate::{ExporterStdoutHelper, SubMsOtelResource};

    let resource = opentelemetry_sdk::Resource::builder_empty()
        .with_attributes(SubMsOtelResource::detect())
        .build();
    let (meter_provider, tracer_provider) = ExporterStdoutHelper::build(resource);
    let _ = meter_provider.meter("test");
    meter_provider.shutdown().ok();
    tracer_provider.shutdown().ok();
}
