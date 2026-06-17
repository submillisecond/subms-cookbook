//! Counter + gauge surface tests.
//! - `subms.bench.ops_total` increments on every on_record.
//! - `subms.bench.in_flight` observes the async observer's queue depth.
//! - `subms.otel.dropped_total` was already covered in observer_async_tests.

#![cfg(feature = "observer")]

mod common;

use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use subms::{ObservationCtx, SubMsObserver, SubMsStageKind};
use subms_otel::{OPS_TOTAL_COUNTER_NAME, OtelObserver, OtelObserverAsync};

use crate::common::{InMemoryMetricExporter, MetricSnapshot, PointSnapshot};

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

fn counter_sum(snapshots: &[Vec<MetricSnapshot>], name: &str) -> u64 {
    let mut sum = 0u64;
    for snap in snapshots {
        for m in snap {
            if m.name == name {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { value, .. } = p {
                        sum += *value;
                    }
                }
            }
        }
    }
    sum
}

fn gauge_max(snapshots: &[Vec<MetricSnapshot>], name: &str) -> Option<u64> {
    let mut seen: Option<u64> = None;
    for snap in snapshots {
        for m in snap {
            if m.name == name {
                for p in &m.points {
                    if let PointSnapshot::GaugeU64 { value, .. } = p {
                        seen = Some(seen.map_or(*value, |old| old.max(*value)));
                    }
                }
            }
        }
    }
    seen
}

#[test]
fn sync_observer_increments_ops_total_per_record() {
    let (exporter, provider) = build_provider();
    let observer = OtelObserver::new(provider.meter("ops-test"));
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in 0..50u64 {
        observer.on_record(&ctx, ns + 1);
    }
    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let total = counter_sum(&snapshots, OPS_TOTAL_COUNTER_NAME);
    assert_eq!(total, 50);
}

#[test]
fn async_observer_increments_ops_total_per_record() {
    let (exporter, provider) = build_provider();
    let observer = OtelObserverAsync::builder(provider.meter("ops-test"))
        .with_capacity(4096)
        .with_drain_interval(Duration::from_millis(10))
        .build();
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in 0..100u64 {
        observer.on_record(&ctx, ns + 1);
    }
    observer.flush();
    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let total = counter_sum(&snapshots, OPS_TOTAL_COUNTER_NAME);
    assert_eq!(total, 100);
}

#[test]
fn in_flight_gauge_observes_queue_depth() {
    let (exporter, provider) = build_provider();
    let observer = OtelObserverAsync::builder(provider.meter("in-flight-test"))
        .with_capacity(1024)
        .with_drain_interval(Duration::from_secs(60))
        .build();
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in 0..200u64 {
        observer.on_record(&ctx, ns + 1);
    }
    let depth_before_flush = observer.in_flight();
    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let gauge =
        gauge_max(&snapshots, "subms.bench.in_flight").expect("in_flight gauge should be present");
    assert!(
        gauge >= 1,
        "expected gauge to observe non-empty queue (depth before flush: {})",
        depth_before_flush
    );
    observer.flush();
}
