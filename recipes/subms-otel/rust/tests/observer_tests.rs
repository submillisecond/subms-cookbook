//! Tests for the synchronous `OtelObserver`. Register against a harness,
//! drive samples + a summarise, then read back the in-memory exporter.

#![cfg(feature = "observer")]

mod common;

use std::sync::Arc;
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use subms::{SubMsObserver, SubMsPerfHarness, SubMsStageKind, summarize};
use subms_otel::{HISTOGRAM_NAME, OtelObserver};

use crate::common::InMemoryMetricExporter;

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

fn total_histogram_count(exporter: &InMemoryMetricExporter, provider: &SdkMeterProvider) -> u64 {
    provider.force_flush().expect("flush");
    let mut total = 0u64;
    for snap in exporter.get_finished_metrics().expect("get") {
        for m in snap {
            if m.name == HISTOGRAM_NAME {
                for p in &m.points {
                    if let Some(c) = p.histogram_count() {
                        total += c;
                    }
                }
            }
        }
    }
    total
}

#[test]
fn on_record_lands_on_the_meter() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-sync-test");
    let observer = Arc::new(OtelObserver::new(meter));
    let mut h = SubMsPerfHarness::new("workload-x", "rust");
    h.set_observer(Some(observer));

    let put = h.stage("put", 10);
    put.with_kind(SubMsStageKind::HotPath);
    for ns in [100u64, 200, 300, 400, 500] {
        put.record(ns);
    }

    let count = total_histogram_count(&exporter, &provider);
    assert_eq!(count, 5, "5 records, 5 hits");
}

#[test]
fn on_summarize_re_emits_percentile_set() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-sync-test");
    let observer: Arc<dyn SubMsObserver> = Arc::new(OtelObserver::new(meter));
    let mut h = SubMsPerfHarness::new("workload-y", "rust");
    h.input("entries", "5");
    h.add_meta("subms.recipe.slug", "subms-x");
    h.set_observer(Some(observer));

    let put = h.stage("put", 5);
    put.with_kind(SubMsStageKind::HotPath);
    for ns in [100u64, 200, 300, 400, 500] {
        put.record(ns);
    }

    let before = total_histogram_count(&exporter, &provider);
    exporter.reset();

    let summary = summarize(&h);
    h.observer().unwrap().on_summarize(&summary);

    let after = total_histogram_count(&exporter, &provider);
    assert!(before >= 5);
    assert!(
        after >= 5,
        "summary should emit percentile records, got {}",
        after
    );
}

#[test]
fn on_summarize_attaches_full_attribute_set() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-sync-test");
    let observer: Arc<dyn SubMsObserver> = Arc::new(OtelObserver::new(meter));
    let mut h = SubMsPerfHarness::new("workload-z", "rust");
    h.input("entries", "1234");
    h.add_meta("host", "ci-runner");
    h.add_meta("hardware_tier", "ci-shared");
    h.set_observer(Some(observer));

    let put = h.stage("put", 3);
    put.with_kind(SubMsStageKind::HotPath);
    put.record(100);
    put.record(200);
    put.record(300);

    provider.force_flush().unwrap();
    exporter.reset();

    let summary = summarize(&h);
    h.observer().unwrap().on_summarize(&summary);
    provider.force_flush().unwrap();

    let snapshots = exporter.get_finished_metrics().expect("get");
    let mut saw_full_attrs = false;
    for snap in &snapshots {
        for m in snap {
            if m.name == HISTOGRAM_NAME {
                for p in &m.points {
                    let keys: Vec<String> = p
                        .attributes()
                        .iter()
                        .map(|kv| kv.key.as_str().to_string())
                        .collect();
                    if keys.contains(&"subms.host".to_string())
                        && keys.contains(&"subms.workload.entries".to_string())
                        && keys.contains(&"subms.hardware.tier".to_string())
                    {
                        saw_full_attrs = true;
                    }
                }
            }
        }
    }
    assert!(
        saw_full_attrs,
        "on_summarize should attach inputs+meta-derived attrs"
    );
}
