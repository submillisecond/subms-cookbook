//! Tests for the always-on bridge: `export_summary`, `export_timer`,
//! `histogram_boundaries`. Uses the hand-rolled in-memory exporters to
//! capture emissions for assertion.

#![cfg(feature = "bridge")]

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::SdkTracerProvider;

use subms::{SubMsBenchSummary, SubMsStageKind, SubMsStageSummary, SubMsTimer};
use subms_otel::{
    HISTOGRAM_NAME, HISTOGRAM_UNIT, export_summary, export_timer, histogram_boundaries,
};

use crate::common::{InMemoryMetricExporter, InMemorySpanExporter, MetricSnapshot};

fn fake_summary() -> SubMsBenchSummary {
    let mut inputs = BTreeMap::new();
    inputs.insert("entries".to_string(), "50000".to_string());
    inputs.insert("seed".to_string(), "42".to_string());
    let mut meta = BTreeMap::new();
    meta.insert(
        "subms.recipe.slug".to_string(),
        "subms-bloom-filter".to_string(),
    );
    meta.insert(
        "subms.recipe.category".to_string(),
        "probabilistic".to_string(),
    );
    meta.insert("host".to_string(), "ci-runner".to_string());
    meta.insert("hardware_tier".to_string(), "ci-shared".to_string());
    meta.insert("crate_version".to_string(), "0.5.0".to_string());

    SubMsBenchSummary {
        workload: "bloom".to_string(),
        lang: "rust".to_string(),
        timestamp: "2026-05-30T00:00:00Z".to_string(),
        inputs,
        meta,
        stages: vec![SubMsStageSummary {
            name: "put".to_string(),
            count: 100,
            p50_ns: 500,
            p99_ns: 900,
            p999_ns: 1500,
            max_ns: 2000,
            mean_ns: 600,
            stddev_ns: 50,
            cdf_buckets_ns: vec![0; 64],
            jitter_score: 0.01,
            samples_ns: Some(vec![400, 500, 600, 700, 800, 900, 1000]),
        }],
    }
}

fn collect_metrics(
    exporter: &InMemoryMetricExporter,
    provider: &SdkMeterProvider,
) -> Vec<Vec<MetricSnapshot>> {
    provider.force_flush().expect("flush");
    exporter
        .get_finished_metrics()
        .expect("get_finished_metrics")
}

#[test]
fn histogram_boundaries_match_kind_table() {
    let hot = histogram_boundaries(SubMsStageKind::HotPath);
    assert_eq!(hot.len(), 12);
    assert!((hot[0] - 5e-8).abs() < 1e-12);
    assert!((hot[11] - 1e-3).abs() < 1e-12);

    let batch = histogram_boundaries(SubMsStageKind::BatchOp);
    assert_eq!(batch.len(), 7);
    assert!((batch[0] - 1e-6).abs() < 1e-12);
    assert!((batch[6] - 1.0).abs() < 1e-12);

    let one_shot = histogram_boundaries(SubMsStageKind::OneShot);
    assert_eq!(one_shot, batch, "OneShot mirrors BatchOp");

    let unspec = histogram_boundaries(SubMsStageKind::Unspecified);
    assert!(unspec.is_empty(), "Unspecified falls back to OTEL default");
}

#[test]
fn export_summary_emits_named_histogram_with_attributes() {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("subms-otel-test");

    let summary = fake_summary();
    export_summary(&summary, &meter);

    let snapshots = collect_metrics(&exporter, &provider);
    let mut found_histogram = false;
    for snap in &snapshots {
        for m in snap {
            if m.name == HISTOGRAM_NAME {
                found_histogram = true;
                assert_eq!(m.unit, HISTOGRAM_UNIT);
                for p in &m.points {
                    let attrs: Vec<KeyValue> = p.attributes().to_vec();
                    assert!(attrs.iter().any(|kv| kv.key.as_str() == "subms.workload"));
                    assert!(attrs.iter().any(|kv| kv.key.as_str() == "subms.lang"));
                    assert!(attrs.iter().any(|kv| kv.key.as_str() == "subms.stage"));
                    assert!(
                        attrs
                            .iter()
                            .any(|kv| kv.key.as_str() == "subms.recipe.slug"),
                        "recipe.slug attribute should be pulled from meta"
                    );
                    assert!(
                        attrs
                            .iter()
                            .any(|kv| kv.key.as_str() == "subms.workload.entries"),
                        "entries attribute should be pulled from inputs"
                    );
                    let count = p.histogram_count().unwrap_or(0);
                    assert!(
                        count >= 5,
                        "expected percentiles + samples to land in the histogram, got count={}",
                        count
                    );
                }
            }
        }
    }
    assert!(
        found_histogram,
        "expected to find a {} histogram",
        HISTOGRAM_NAME
    );
}

#[test]
fn export_summary_handles_empty_samples_block() {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("subms-otel-test");

    let mut summary = fake_summary();
    summary.stages[0].samples_ns = None;
    export_summary(&summary, &meter);

    let snapshots = collect_metrics(&exporter, &provider);
    let mut total_count = 0u64;
    for snap in &snapshots {
        for m in snap {
            if m.name == HISTOGRAM_NAME {
                for p in &m.points {
                    total_count += p.histogram_count().unwrap_or(0);
                }
            }
        }
    }
    assert!(
        total_count >= 5,
        "expected at least 5 percentile records, got {}",
        total_count
    );
}

#[test]
fn export_timer_emits_parent_and_child_spans() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let tracer = provider.tracer("subms-otel-test");

    let mut timer = SubMsTimer::new("parse-request");
    std::thread::sleep(Duration::from_millis(1));
    timer.mark("headers");
    std::thread::sleep(Duration::from_millis(1));
    timer.mark("body");
    std::thread::sleep(Duration::from_millis(1));
    timer.stop("served");

    export_timer(&timer, &tracer);

    let spans = exporter.get_finished_spans().expect("get_finished_spans");
    assert!(!spans.is_empty(), "expected at least one span");

    let parent_count = spans.iter().filter(|s| s.name == "parse-request").count();
    assert_eq!(
        parent_count, 1,
        "exactly one parent span named after the timer"
    );

    let label_names: Vec<&str> = spans.iter().map(|s| s.name.as_ref()).collect();
    assert!(label_names.contains(&"headers"));
    assert!(label_names.contains(&"body"));
    assert!(label_names.contains(&"served"));

    let served = spans
        .iter()
        .find(|s| s.name == "served")
        .expect("served span");
    let is_stop_attr = served
        .attributes
        .iter()
        .find(|kv| kv.key.as_str() == "subms.timer.is_stop")
        .expect("subms.timer.is_stop attribute");
    assert_eq!(is_stop_attr.value.as_str(), "true");
}

#[test]
fn export_timer_handles_unnamed_timer() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let tracer = provider.tracer("subms-otel-test");

    let mut timer = SubMsTimer::unnamed();
    timer.mark("a");
    timer.stop("b");
    export_timer(&timer, &tracer);

    let spans = exporter.get_finished_spans().expect("get_finished_spans");
    let parent = spans
        .iter()
        .find(|s| s.name == "subms.timer")
        .expect("default parent span name");
    assert!(
        parent
            .attributes
            .iter()
            .any(|kv| kv.key.as_str() == "subms.timer.checkpoints")
    );
}
