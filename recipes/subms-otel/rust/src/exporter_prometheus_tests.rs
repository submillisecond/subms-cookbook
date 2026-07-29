//! Smoke: the Prometheus exporter helper builds a `(MeterProvider, exporter)`
//! pair and the exporter scrapes the Prom text format.

use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::Resource;

use crate::ExporterPrometheusHelper;

fn fake_resource() -> Resource {
    Resource::builder_empty()
        .with_attributes([KeyValue::new("service.name", "subms-otel-test")])
        .build()
}

#[test]
fn helper_builds_and_records() {
    let (mp, exporter) =
        ExporterPrometheusHelper::build(fake_resource()).expect("prom build should succeed");
    let meter = mp.meter("smoke");
    let h = meter.f64_histogram("smoke.latency").with_unit("s").build();
    h.record(0.0001, &[KeyValue::new("stage", "put")]);
    mp.force_flush().expect("flush");

    let text = exporter.scrape();
    assert!(
        text.contains("smoke_latency_seconds_bucket"),
        "got:\n{}",
        text
    );
    assert!(text.contains("smoke_latency_seconds_count"));
    assert!(text.contains("smoke_latency_seconds_sum"));
    assert!(text.contains("le=\"+Inf\""));
    assert!(text.contains("stage=\"put\""));
}

#[test]
fn counter_renders_with_total_suffix() {
    let (mp, exporter) =
        ExporterPrometheusHelper::build(fake_resource()).expect("prom build should succeed");
    let meter = mp.meter("smoke");
    let c = meter.u64_counter("subms.bench.ops").build();
    c.add(7, &[KeyValue::new("subms.stage", "put")]);
    mp.force_flush().expect("flush");

    let text = exporter.scrape();
    assert!(text.contains("subms_bench_ops_total"), "got:\n{}", text);
    assert!(text.contains("subms_stage=\"put\""));
    assert!(text.contains(" 7"));
}

#[test]
fn gauge_renders_without_total_suffix() {
    let (mp, exporter) =
        ExporterPrometheusHelper::build(fake_resource()).expect("prom build should succeed");
    let meter = mp.meter("smoke");
    let g = meter.u64_observable_gauge("subms.queue.depth").build();
    let _ = g; // observed via the SDK's pull, not driven directly here
    let _ = meter
        .u64_observable_gauge("subms.bench.in_flight")
        .with_callback(|obs| obs.observe(3, &[]))
        .build();
    mp.force_flush().expect("flush");

    let text = exporter.scrape();
    assert!(text.contains("subms_bench_in_flight"), "got:\n{}", text);
    assert!(!text.contains("subms_bench_in_flight_total"));
}

#[test]
fn dots_become_underscores_in_attr_keys() {
    let (mp, exporter) =
        ExporterPrometheusHelper::build(fake_resource()).expect("prom build should succeed");
    let meter = mp.meter("smoke");
    let h = meter.f64_histogram("subms.latency").with_unit("s").build();
    h.record(
        0.000001,
        &[
            KeyValue::new("subms.stage", "put"),
            KeyValue::new("subms.recipe.slug", "subms-bloom-filter"),
        ],
    );
    mp.force_flush().expect("flush");
    let text = exporter.scrape();
    assert!(text.contains("subms_recipe_slug=\"subms-bloom-filter\""));
    assert!(text.contains("subms_stage=\"put\""));
    assert!(text.contains("subms_latency_seconds_bucket"));
}
