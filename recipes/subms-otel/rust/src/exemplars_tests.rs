//! Exemplar reservoir tests: keeps slowest K, attributes match expectation,
//! sidecar gauge fires on publish.

use std::sync::Arc;
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use subms::{ObservationCtx, SubMsObserver, SubMsStageKind};

use crate::test_common::{InMemoryMetricExporter, PointSnapshot};
use crate::{EXEMPLAR_GAUGE_NAME, ExemplarReservoir, OtelObserver};

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

#[test]
fn reservoir_keeps_slowest_k_distinct_samples() {
    let r = ExemplarReservoir::with_capacity(3);
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in [1u64, 5, 10, 2, 3, 8, 7, 4] {
        r.offer(&ctx, ns);
    }
    let snap = r.snapshot();
    assert_eq!(snap.len(), 3);
    let kept: Vec<u64> = snap.iter().map(|e| e.ns).collect();
    assert!(kept.contains(&10));
    assert!(kept.contains(&8));
    assert!(kept.contains(&7));
}

#[test]
fn reservoir_offers_capture_full_attribute_set() {
    let r = ExemplarReservoir::with_capacity(2);
    let ctx = ObservationCtx {
        workload: "bloom",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    r.offer(&ctx, 100);
    let snap = r.snapshot();
    assert_eq!(snap.len(), 1);
    let attr_keys: Vec<&str> = snap[0]
        .attributes
        .iter()
        .map(|kv| kv.key.as_str())
        .collect();
    assert!(attr_keys.contains(&"subms.workload"));
    assert!(attr_keys.contains(&"subms.stage"));
    assert!(attr_keys.contains(&"subms.stage.kind"));
    assert!(attr_keys.contains(&"subms.lang"));
}

#[test]
fn reservoir_publish_emits_subms_exemplars_gauge() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("exemplar-test");
    let r = ExemplarReservoir::with_capacity(3);
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    r.offer(&ctx, 500);
    r.offer(&ctx, 1500);
    r.offer(&ctx, 2500);
    r.publish(&meter);

    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let mut saw_gauge = false;
    for snap in &snapshots {
        for m in snap {
            if m.name == EXEMPLAR_GAUGE_NAME {
                saw_gauge = true;
                assert!(!m.points.is_empty(), "expected at least 1 exemplar point");
            }
        }
    }
    assert!(saw_gauge, "exemplar gauge should appear in export");
}

#[test]
fn observer_with_reservoir_increments_exemplars_kept_counter() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("exemplar-test");
    let reservoir = Arc::new(ExemplarReservoir::with_capacity(2));
    let observer = OtelObserver::new(meter).with_exemplar_reservoir(Arc::clone(&reservoir));

    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    };
    for ns in [10u64, 20, 30, 40, 50] {
        observer.on_record(&ctx, ns);
    }

    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let mut kept_counter_total = 0u64;
    for snap in &snapshots {
        for m in snap {
            if m.name == "subms.otel.exemplars_kept_total" {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { value, .. } = p {
                        kept_counter_total += value;
                    }
                }
            }
        }
    }
    assert!(
        kept_counter_total >= 2,
        "expected the kept-exemplars counter to fire >= 2 times, got {}",
        kept_counter_total
    );
}
