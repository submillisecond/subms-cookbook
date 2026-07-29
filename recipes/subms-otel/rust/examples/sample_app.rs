//! Sample app: a tour of `subms-otel`, the OpenTelemetry bridge, base API
//! first then each optional feature. The scenario is a trading gateway whose
//! health flips + lifecycle events are exported to an OpenTelemetry collector,
//! with the adapter's per-record observer overhead standing in as the sub-ms
//! claim.
//!
//! Run the base with `cargo run --example sample_app`; add `--all-features`
//! (or a subset like `--features observer,exporter-prometheus`) to light up the
//! feature sections.
//!
//! * base                - health/event forwarding via the always-on bridge
//! * observer            - the live per-record histogram observer (the hot path)
//! * exemplars           - slow-sample retention per latency bucket
//! * tracing             - one span per recorded op, W3C parent inheritance
//! * autoconfig          - env-driven one-line wiring + observer registry
//! * exporter-otlp       - build an OTLP-exporting MeterProvider
//! * exporter-prometheus - see the exact Prometheus scrape bytes
//! * exporter-stdout     - JSON-per-metric to stdout for local debugging

use std::sync::Arc;

use subms_otel::OtelEventBridge;

fn main() {
    base_gateway_health_events();

    #[cfg(feature = "observer")]
    observer_hot_path();

    #[cfg(feature = "exemplars")]
    exemplars_slow_tail();

    #[cfg(feature = "tracing")]
    tracing_spans();

    #[cfg(feature = "autoconfig")]
    autoconfig_registry();

    #[cfg(feature = "exporter-otlp")]
    exporter_otlp();

    #[cfg(feature = "exporter-prometheus")]
    exporter_prometheus();

    #[cfg(feature = "exporter-stdout")]
    exporter_stdout();
}

/// Base bridge (default features): a trading gateway forwards its health
/// transitions and lifecycle events to OpenTelemetry. `OtelEventBridge` plugs
/// into a `subms-events` bus and turns each event into a `subms.events.total`
/// counter; `StateTransitionRecorder` gives the health flips a dedicated
/// transition counter. Both read the globally-installed MeterProvider.
fn base_gateway_health_events() {
    use subms_events::{Event, EventDispatcher, EventLevel};
    use subms_otel::StateTransitionRecorder;

    println!("== base: gateway health + events to OTel ==");

    let exporter = capture::CountingExporter::default();
    let provider = capture::meter_provider(exporter.clone());
    opentelemetry::global::set_meter_provider(provider.clone());

    let health =
        StateTransitionRecorder::new("gateway.health.transitions", "trading gateway health flips");

    let mut bus = EventDispatcher::sync();
    bus.add_bridge(Arc::new(OtelEventBridge::new()));

    let lifecycle = [
        Event::transition(
            "gateway.health",
            EventLevel::Warn,
            "gateway",
            "UP",
            "DEGRADED",
        ),
        Event::builder("order.rejected")
            .attr("venue", "XNAS")
            .build(),
        Event::transition(
            "gateway.health",
            EventLevel::Info,
            "gateway",
            "DEGRADED",
            "UP",
        ),
    ];
    for ev in &lifecycle {
        bus.emit(ev.clone());
    }
    health.record(
        "gateway",
        "UP",
        "DEGRADED",
        &[("reason", "spread_widened".to_string())],
    );
    health.record("gateway", "DEGRADED", "UP", &[]);

    provider.force_flush().expect("flush");
    let events_total = exporter.counter_total("subms.events.total");
    let flips = exporter.counter_total("gateway.health.transitions");
    println!("  forwarded {events_total} events, {flips} health flips to OTel");
    assert_eq!(
        events_total, 3,
        "every emitted event reaches the OTel counter"
    );
    assert_eq!(
        flips, 2,
        "both health transitions land on the transition counter"
    );
}

/// `observer` feature: the live per-record path. `OtelObserver::on_record` is
/// what the harness calls on every sampled op; it records one histogram point
/// and bumps `subms.bench.ops_total`. That per-record cost is the recipe's
/// sub-ms claim, so the tour drives a batch of hot-path records through it.
#[cfg(feature = "observer")]
fn observer_hot_path() {
    use opentelemetry::metrics::MeterProvider;
    use subms_otel::{ObservationCtx, OtelObserver, SubMsObserver, SubMsStageKind};

    println!("\n== observer: live per-record histogram ==");

    let exporter = capture::CountingExporter::default();
    let provider = capture::meter_provider(exporter.clone());
    let observer = OtelObserver::new(provider.meter("subms-otel"));

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
    let ops = exporter.counter_total("subms.bench.ops_total");
    println!("  {ops} hot-path records emitted a histogram point each");
    assert_eq!(ops, 5, "one ops_total increment per on_record");
}

/// `exemplars` feature: keep the slowest K samples per latency bucket so a
/// dashboard can jump from a p99 spike straight to the offending op. No SDK
/// readback needed - the reservoir's retained set is directly inspectable.
#[cfg(feature = "exemplars")]
fn exemplars_slow_tail() {
    use subms_otel::{ExemplarReservoir, ObservationCtx, SubMsStageKind};

    println!("\n== exemplars: slowest-K tail retention ==");

    let reservoir = ExemplarReservoir::with_capacity(3);
    let ctx = ObservationCtx {
        workload: "gateway-submit",
        lang: "rust",
        stage: "submit",
        stage_kind: SubMsStageKind::HotPath,
    };
    // All five fall in the same hot-path bucket (500ns .. 1us); the reservoir
    // keeps the slowest K of them.
    for ns in [600u64, 950, 700, 900, 800] {
        reservoir.offer(&ctx, ns);
    }
    let mut kept: Vec<u64> = reservoir.snapshot().iter().map(|e| e.ns).collect();
    kept.sort_unstable();
    println!("  kept slowest {} of 5: {:?}", kept.len(), kept);
    assert_eq!(
        kept,
        vec![800, 900, 950],
        "slowest three in the bucket retained"
    );
}

/// `tracing` feature: emit one span per recorded op, its start reconstructed as
/// `now - ns`, inheriting any active W3C parent. A production request's trace
/// then flows through the bench instrumentation as child spans.
#[cfg(feature = "tracing")]
fn tracing_spans() {
    use opentelemetry::trace::TracerProvider;
    use subms_otel::{
        ObservationCtx, SubMsObserver, SubMsStageKind, TRACING_SPAN_NAME, TracingObserver,
    };

    println!("\n== tracing: one span per record ==");

    let exporter = capture::SpanCollector::default();
    let provider = capture::tracer_provider(exporter.clone());
    let observer = TracingObserver::new(provider.tracer("subms-otel"));

    let ctx = ObservationCtx {
        workload: "gateway-submit",
        lang: "rust",
        stage: "submit",
        stage_kind: SubMsStageKind::HotPath,
    };
    observer.on_record(&ctx, 240);
    observer.on_record(&ctx, 310);

    let names = exporter.span_names();
    println!("  emitted {} spans named {TRACING_SPAN_NAME}", names.len());
    assert_eq!(names.len(), 2, "one span per record");
    assert!(names.iter().all(|n| n == TRACING_SPAN_NAME));
}

/// `autoconfig` feature: an application registers its observer once at startup
/// and `auto_configure()` reads the standard OTEL_* / SUBMS_OTEL_* env, builds
/// providers, and prefers the registered observer over a fresh build.
#[cfg(feature = "autoconfig")]
fn autoconfig_registry() {
    use subms_otel::{
        OtelObserver, auto_configure, clear_registered_observers, register_observer,
        registered_observers,
    };

    println!("\n== autoconfig: env-driven wiring + registry ==");

    clear_registered_observers();
    let cfg = auto_configure();
    register_observer(
        "gateway-otel",
        Arc::new(OtelObserver::new(cfg.meter.clone())),
    );
    let registered = registered_observers();
    println!(
        "  {} observer(s) registered; auto_configure wired a meter + tracer",
        registered.len()
    );
    assert_eq!(
        registered.len(),
        1,
        "the registered observer is discoverable"
    );

    cfg.meter_provider.shutdown().ok();
    cfg.tracer_provider.shutdown().ok();
    clear_registered_observers();
}

/// `exporter-otlp` feature: build an OTLP-exporting MeterProvider. The builder
/// constructs the exporter lazily (no live collector needed here); a real
/// deployment points `with_endpoint` at its collector.
#[cfg(feature = "exporter-otlp")]
fn exporter_otlp() {
    use subms_otel::{OtlpBuilder, OtlpProtocol};

    println!("\n== exporter-otlp: OTLP MeterProvider ==");

    assert_eq!(OtlpProtocol::from_env(Some("grpc")), OtlpProtocol::Grpc);
    assert_eq!(OtlpProtocol::from_env(None), OtlpProtocol::HttpProtobuf);

    let provider = OtlpBuilder::new()
        .with_endpoint("http://localhost:4318")
        .build();
    println!("  OTLP/HTTP MeterProvider built: {}", provider.is_some());
    assert!(
        provider.is_some(),
        "the OTLP exporter builds against the endpoint"
    );
    if let Some(p) = provider {
        p.shutdown().ok();
    }
}

/// `exporter-prometheus` feature: the one section that shows the exact bytes a
/// collector scrapes. Wire the in-tree text exporter, record a bench summary,
/// flush, and read the Prometheus exposition off `scrape()`.
#[cfg(feature = "exporter-prometheus")]
fn exporter_prometheus() {
    use opentelemetry::metrics::MeterProvider;
    use subms_otel::{PrometheusBuilder, export_summary};

    println!("\n== exporter-prometheus: scrape bytes ==");

    let (provider, exporter) = PrometheusBuilder::new()
        .build()
        .expect("prometheus provider");
    let meter = provider.meter("subms-otel");
    export_summary(&fake_summary(), &meter);
    provider.force_flush().expect("flush");

    let text = exporter.scrape();
    let first = text.lines().next().unwrap_or("");
    println!("  scrape() first line: {first}");
    assert!(
        text.contains("subms_latency_seconds"),
        "histogram lands in Prometheus text"
    );
    assert!(
        text.contains("subms_stage=\"submit\""),
        "stage attribute becomes a label"
    );
}

/// `exporter-stdout` feature: the autoconfig fallback. Emits one JSON line per
/// metric to stdout - handy for eyeballing emissions during local debugging.
#[cfg(feature = "exporter-stdout")]
fn exporter_stdout() {
    use opentelemetry::metrics::MeterProvider;
    use subms_otel::{ExporterStdoutHelper, SubMsOtelResource};

    println!("\n== exporter-stdout: JSON per metric ==");

    let resource = opentelemetry_sdk::Resource::builder_empty()
        .with_attributes(SubMsOtelResource::detect())
        .build();
    let (meter_provider, tracer_provider) = ExporterStdoutHelper::build(resource);

    let counter = meter_provider
        .meter("subms-otel")
        .u64_counter("subms.demo.stdout")
        .build();
    counter.add(1, &[]);
    println!("  emitting one metric line below:");
    meter_provider.force_flush().expect("flush");

    meter_provider.shutdown().ok();
    tracer_provider.shutdown().ok();
}

#[cfg(feature = "exporter-prometheus")]
fn fake_summary() -> subms_otel::SubMsBenchSummary {
    use std::collections::BTreeMap;
    use subms_otel::{SubMsBenchSummary, SubMsStageSummary};

    let mut inputs = BTreeMap::new();
    inputs.insert("entries".to_string(), "50000".to_string());
    let mut meta = BTreeMap::new();
    meta.insert("subms.recipe.slug".to_string(), "subms-otel".to_string());

    SubMsBenchSummary {
        workload: "gateway-submit".to_string(),
        lang: "rust".to_string(),
        timestamp: "2026-07-28T00:00:00Z".to_string(),
        cpu_core: None,
        cpu_affinity: None,
        inputs,
        meta,
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
            jitter_score: 0.01,
            samples_ns: Some(vec![120, 180, 240, 160, 900]),
        }],
    }
}

/// In-memory OTEL exporters used only by this sample so each section can verify
/// its own emission without a live collector. `opentelemetry_sdk` is a
/// dev-dependency, always available to the example regardless of features.
#[allow(dead_code)]
mod capture {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
    use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
    use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

    #[derive(Clone, Default)]
    pub struct CountingExporter {
        totals: Arc<Mutex<HashMap<String, u64>>>,
    }

    impl CountingExporter {
        pub fn counter_total(&self, name: &str) -> u64 {
            *self
                .totals
                .lock()
                .expect("totals poisoned")
                .get(name)
                .unwrap_or(&0)
        }
    }

    impl PushMetricExporter for CountingExporter {
        async fn export(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
            let mut totals = self.totals.lock().expect("totals poisoned");
            for sm in metrics.scope_metrics() {
                for m in sm.metrics() {
                    if let AggregatedMetrics::U64(MetricData::Sum(s)) = m.data() {
                        let sum: u64 = s.data_points().map(|dp| dp.value()).sum();
                        *totals.entry(m.name().to_string()).or_insert(0) += sum;
                    }
                }
            }
            Ok(())
        }
        fn force_flush(&self) -> OTelSdkResult {
            Ok(())
        }
        fn shutdown_with_timeout(&self, _t: Duration) -> OTelSdkResult {
            Ok(())
        }
        fn temporality(&self) -> Temporality {
            Temporality::Cumulative
        }
    }

    pub fn meter_provider(exporter: CountingExporter) -> SdkMeterProvider {
        let reader = PeriodicReader::builder(exporter)
            .with_interval(Duration::from_secs(60))
            .build();
        SdkMeterProvider::builder().with_reader(reader).build()
    }

    #[cfg(feature = "tracing")]
    pub use span::{SpanCollector, tracer_provider};

    #[cfg(feature = "tracing")]
    mod span {
        use std::sync::{Arc, Mutex};

        use opentelemetry_sdk::error::OTelSdkResult;
        use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter};

        #[derive(Debug, Clone, Default)]
        pub struct SpanCollector {
            names: Arc<Mutex<Vec<String>>>,
        }

        impl SpanCollector {
            pub fn span_names(&self) -> Vec<String> {
                self.names.lock().expect("names poisoned").clone()
            }
        }

        impl SpanExporter for SpanCollector {
            async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
                let mut names = self.names.lock().expect("names poisoned");
                names.extend(batch.into_iter().map(|s| s.name.to_string()));
                Ok(())
            }
        }

        pub fn tracer_provider(exporter: SpanCollector) -> SdkTracerProvider {
            SdkTracerProvider::builder()
                .with_simple_exporter(exporter)
                .build()
        }
    }
}
