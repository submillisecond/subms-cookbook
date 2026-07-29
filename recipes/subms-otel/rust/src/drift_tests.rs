//! Reference-impl divergence counter tests.

use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use subms::SubMsStageKind;

use crate::test_common::{InMemoryMetricExporter, PointSnapshot};
use crate::{
    REFERENCE_DIVERGENCE_COUNTER_NAME, ReferenceDivergenceRecorder, divergence_attributes,
    record_reference_divergence,
};

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

#[test]
fn divergence_attributes_carry_expected_keys() {
    let attrs = divergence_attributes(
        "contains",
        SubMsStageKind::HotPath,
        "set_membership",
        "true",
        "false",
    );
    let keys: Vec<&str> = attrs.iter().map(|kv| kv.key.as_str()).collect();
    assert!(keys.contains(&"subms.stage"));
    assert!(keys.contains(&"subms.stage.kind"));
    assert!(keys.contains(&"subms.reference.kind"));
    assert!(keys.contains(&"subms.reference.expected"));
    assert!(keys.contains(&"subms.reference.observed"));
}

#[test]
fn record_reference_divergence_fires_counter() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("drift-test");
    record_reference_divergence(
        &meter,
        "contains",
        SubMsStageKind::HotPath,
        "set_membership",
        "true",
        "false",
    );
    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let mut total = 0u64;
    let mut saw_attrs = false;
    for snap in &snapshots {
        for m in snap {
            if m.name == REFERENCE_DIVERGENCE_COUNTER_NAME {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { attributes, value } = p {
                        total += value;
                        let attr_keys: Vec<&str> =
                            attributes.iter().map(|kv| kv.key.as_str()).collect();
                        if attr_keys.contains(&"subms.reference.kind")
                            && attr_keys.contains(&"subms.stage")
                        {
                            saw_attrs = true;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(total, 1);
    assert!(saw_attrs);
}

#[test]
fn recorder_reuses_counter_across_calls() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("drift-test");
    let recorder = ReferenceDivergenceRecorder::new(meter);
    for _ in 0..5 {
        recorder.record(
            "estimate",
            SubMsStageKind::HotPath,
            "count_estimate",
            "1000",
            "997",
        );
    }
    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let mut total = 0u64;
    for snap in &snapshots {
        for m in snap {
            if m.name == REFERENCE_DIVERGENCE_COUNTER_NAME {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { value, .. } = p {
                        total += value;
                    }
                }
            }
        }
    }
    assert_eq!(total, 5);
}

#[cfg(feature = "observer")]
#[test]
fn sync_observer_record_reference_divergence_works() {
    use subms::SubMsObserver;

    use crate::OtelObserver;
    let (exporter, provider) = build_provider();
    let observer = OtelObserver::new(provider.meter("drift-test"));
    observer.record_reference_divergence(
        "lookup",
        SubMsStageKind::HotPath,
        "set_membership",
        "true",
        "false",
    );
    let _: &dyn SubMsObserver = &observer;
    provider.force_flush().unwrap();
    let snapshots = exporter.get_finished_metrics().unwrap();
    let mut total = 0u64;
    for snap in &snapshots {
        for m in snap {
            if m.name == REFERENCE_DIVERGENCE_COUNTER_NAME {
                for p in &m.points {
                    if let PointSnapshot::SumU64 { value, .. } = p {
                        total += value;
                    }
                }
            }
        }
    }
    assert_eq!(total, 1);
}
