//! Tests for the asynchronous `OtelObserverAsync`.

use std::sync::Arc;
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use subms::{ObservationCtx, SubMsObserver, SubMsStageKind};

use crate::OtelObserverAsync;
use crate::test_common::{InMemoryMetricExporter, PointSnapshot};

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

fn snapshot(exporter: &InMemoryMetricExporter, provider: &SdkMeterProvider) -> (u64, u64) {
    provider.force_flush().expect("flush");
    let mut hist_total = 0u64;
    let mut dropped_total = 0u64;
    let snapshots = exporter.get_finished_metrics().expect("get");
    if let Some(snap) = snapshots.last() {
        for m in snap {
            if m.name == "subms.latency" {
                for p in &m.points {
                    if let Some(c) = p.histogram_count() {
                        hist_total += c;
                    }
                }
            } else if m.name == "subms.otel.dropped_total" {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { value, .. } = p {
                        dropped_total += *value;
                    }
                }
            }
        }
    }
    (hist_total, dropped_total)
}

fn total_histogram_count(
    exporter: &InMemoryMetricExporter,
    provider: &SdkMeterProvider,
    _name_prefix: &str,
) -> u64 {
    snapshot(exporter, provider).0
}

#[test]
fn drains_all_samples_at_moderate_rate() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-async-test");
    let observer = OtelObserverAsync::builder(meter)
        .with_capacity(8192)
        .with_drain_interval(Duration::from_millis(10))
        .build();

    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };

    for ns in 0..5_000u64 {
        observer.on_record(&ctx, ns + 1);
    }

    observer.flush();

    let count = total_histogram_count(&exporter, &provider, "subms.latency");
    assert_eq!(count, 5_000, "every sample should make it through");
    assert_eq!(observer.dropped_count(), 0, "no back-pressure expected");
}

#[test]
fn back_pressure_drops_oldest_and_counter_increments() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-async-test");
    let observer = OtelObserverAsync::builder(meter)
        .with_capacity(16)
        .with_drain_interval(Duration::from_secs(60))
        .build();

    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };

    let n = 10_000u64;
    for i in 0..n {
        observer.on_record(&ctx, i + 1);
    }

    observer.flush();

    let dropped_atomic = observer.dropped_count();
    assert!(
        dropped_atomic > 0,
        "expected drops under back-pressure, got {}",
        dropped_atomic
    );

    let (total_emitted, dropped_via_counter) = snapshot(&exporter, &provider);
    assert_eq!(
        total_emitted + dropped_via_counter,
        n,
        "every sample is either emitted or counted as dropped (emitted={}, dropped={})",
        total_emitted,
        dropped_via_counter
    );
    assert_eq!(
        dropped_via_counter, dropped_atomic,
        "counter and atomic should agree"
    );
}

#[test]
fn on_summarize_caches_inputs_and_meta_for_drain_pass() {
    use subms::{SubMsPerfHarness, summarize};

    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-async-test");
    let observer: Arc<dyn SubMsObserver> = Arc::new(
        OtelObserverAsync::builder(meter)
            .with_capacity(1024)
            .with_drain_interval(Duration::from_millis(10))
            .build(),
    );

    let mut h = SubMsPerfHarness::new("wl", "rust");
    h.input("entries", "5000");
    h.add_meta("host", "ci-runner");
    h.set_observer(Some(Arc::clone(&observer)));

    let put = h.stage("put", 5);
    put.with_kind(SubMsStageKind::HotPath);
    for ns in [10u64, 20, 30, 40, 50] {
        put.record(ns);
    }

    let summary = summarize(&h);
    h.observer().unwrap().on_summarize(&summary);

    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in 0..100u64 {
        observer.on_record(&ctx, ns + 1);
    }

    std::thread::sleep(Duration::from_millis(150));

    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().expect("get");
    let mut saw_host_attr = false;
    for snap in &snapshots {
        for m in snap {
            if m.name == "subms.latency" {
                for p in &m.points {
                    if p.attributes()
                        .iter()
                        .any(|kv| kv.key.as_str() == "subms.host")
                    {
                        saw_host_attr = true;
                    }
                }
            }
        }
    }
    assert!(
        saw_host_attr,
        "post-summarize records should carry the cached meta attribute"
    );
}

#[test]
fn flush_is_idempotent() {
    let (_exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-async-test");
    let observer = OtelObserverAsync::new(meter);
    observer.flush();
    observer.flush();
}
