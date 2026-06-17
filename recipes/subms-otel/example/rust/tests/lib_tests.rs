//! Tests for the subms-otel recipe example surface. Cover TinyMap
//! correctness, workload execution, and end-to-end observer wiring against an
//! in-memory OTel exporter.

use std::sync::Arc;
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{
    InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
    data::{AggregatedMetrics, MetricData},
};

use subms::{SubMsPerfHarness, summarize};
use subms_otel_example::{
    HISTOGRAM_NAME, OtelObserver, OtelObserverAsync, RECIPE_CATEGORY, RECIPE_SLUG, TinyMap,
    WorkloadParams, run_workload, standard_harness,
};

// --- TinyMap correctness --------------------------------------------------

#[test]
fn tiny_map_inserts_and_reads_back() {
    let mut m = TinyMap::with_capacity(16);
    assert!(m.is_empty());
    assert_eq!(m.insert(1, 10), None);
    assert_eq!(m.insert(2, 20), None);
    assert_eq!(m.insert(3, 30), None);
    assert_eq!(m.len(), 3);
    assert_eq!(m.get(1), Some(10));
    assert_eq!(m.get(2), Some(20));
    assert_eq!(m.get(3), Some(30));
}

#[test]
fn tiny_map_overwrites_existing_key() {
    let mut m = TinyMap::with_capacity(16);
    assert_eq!(m.insert(7, 100), None);
    assert_eq!(m.insert(7, 200), Some(100));
    assert_eq!(m.get(7), Some(200));
    assert_eq!(m.len(), 1, "overwrite must not bump len");
}

#[test]
fn tiny_map_returns_none_on_miss() {
    let m = TinyMap::with_capacity(16);
    assert_eq!(m.get(42), None);
    let mut m2 = TinyMap::with_capacity(16);
    m2.insert(1, 10);
    assert_eq!(m2.get(2), None, "absent key should miss cleanly");
}

#[test]
fn tiny_map_capacity_is_power_of_two() {
    let m = TinyMap::with_capacity(5);
    assert!(m.capacity().is_power_of_two());
    assert!(m.capacity() >= 5);
}

// --- workload + stages ----------------------------------------------------

#[test]
fn workload_records_three_hotpath_stages() {
    let params = WorkloadParams {
        entries: 100,
        capacity: 256,
        seed: 0xABCD,
    };
    let h = standard_harness(params);

    let stages: Vec<&str> = h.stages().iter().map(|s| s.name()).collect();
    assert_eq!(stages, vec!["put", "get_hit", "get_miss"]);

    for name in ["put", "get_hit", "get_miss"] {
        let stage = h.stage_by_name(name).expect("stage present");
        assert_eq!(
            stage.samples().len(),
            params.entries as usize,
            "{name} should record one sample per entry",
        );
    }
}

#[test]
fn workload_sets_recipe_meta() {
    let h = standard_harness(WorkloadParams {
        entries: 50,
        capacity: 128,
        seed: 1,
    });
    assert_eq!(
        h.meta().get("subms.recipe.slug").map(String::as_str),
        Some(RECIPE_SLUG),
    );
    assert_eq!(
        h.meta().get("subms.recipe.category").map(String::as_str),
        Some(RECIPE_CATEGORY),
    );
}

// --- OTEL wiring ----------------------------------------------------------

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

fn latency_count_and_attrs(
    exporter: &InMemoryMetricExporter,
    provider: &SdkMeterProvider,
) -> (u64, Vec<Vec<(String, String)>>) {
    provider.force_flush().expect("flush");
    let mut total = 0u64;
    let mut attr_sets: Vec<Vec<(String, String)>> = Vec::new();
    let snapshots = exporter.get_finished_metrics().expect("get metrics");
    if let Some(rm) = snapshots.last() {
        for sm in rm.scope_metrics() {
            for m in sm.metrics() {
                if m.name() != HISTOGRAM_NAME {
                    continue;
                }
                if let AggregatedMetrics::F64(MetricData::Histogram(h)) = m.data() {
                    for dp in h.data_points() {
                        total += dp.count();
                        let attrs: Vec<(String, String)> = dp
                            .attributes()
                            .map(|kv| (kv.key.as_str().to_string(), kv.value.to_string()))
                            .collect();
                        attr_sets.push(attrs);
                    }
                }
            }
        }
    }
    (total, attr_sets)
}

#[test]
fn sync_observer_lands_every_sample_on_the_meter() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-example-sync-test");
    let observer = Arc::new(OtelObserver::new(meter));

    let params = WorkloadParams {
        entries: 200,
        capacity: 512,
        seed: 0xC0FFEE,
    };
    let h = SubMsPerfHarness::new("subms-otel-example", "rust").with_observer(observer);
    let h = run_workload(h, params);

    let (count, _) = latency_count_and_attrs(&exporter, &provider);
    // Three stages, `entries` samples each.
    assert!(
        count >= (3 * params.entries) as u64,
        "expected at least {} hits, got {count}",
        3 * params.entries,
    );
    // Keep the harness alive past the assertion so the observer's cache
    // doesn't get torn down mid-flush.
    drop(h);
}

#[test]
fn async_observer_drains_every_sample_after_flush() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-example-async-test");
    let observer = Arc::new(
        OtelObserverAsync::builder(meter)
            .with_capacity(65_536)
            .with_drain_interval(Duration::from_millis(10))
            .build(),
    );

    let params = WorkloadParams {
        entries: 200,
        capacity: 512,
        seed: 0xCAFEBABE,
    };
    let h = SubMsPerfHarness::new("subms-otel-example", "rust").with_observer(observer.clone());
    let h = run_workload(h, params);

    // Drain before flushing the provider; the worker is otherwise on a 10ms
    // tick and the final batch might still be in flight.
    observer.flush();

    let (count, _) = latency_count_and_attrs(&exporter, &provider);
    // Channel was sized 65k, well above 3 * 200, so nothing should drop.
    assert_eq!(
        observer.dropped_count(),
        0,
        "async observer dropped samples - channel undersized?",
    );
    assert!(
        count >= (3 * params.entries) as u64,
        "async drain should land >= {} hits, got {count}",
        3 * params.entries,
    );
    drop(h);
}

#[test]
fn summary_emission_carries_recipe_attributes() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-example-summary-test");
    let observer: Arc<dyn subms_otel_example::SubMsObserver> = Arc::new(OtelObserver::new(meter));

    let params = WorkloadParams {
        entries: 100,
        capacity: 256,
        seed: 0xFEED_FACE,
    };
    let h = SubMsPerfHarness::new("subms-otel-example", "rust").with_observer(observer);
    let h = run_workload(h, params);

    // Discard the per-record emissions so we can inspect summary-time attrs
    // in isolation.
    provider.force_flush().expect("flush");
    exporter.reset();

    let summary = summarize(&h);
    h.observer().unwrap().on_summarize(&summary);
    provider.force_flush().expect("flush");

    let (_count, attr_sets) = latency_count_and_attrs(&exporter, &provider);
    let mut saw_full_attrs = false;
    for attrs in &attr_sets {
        let keys: Vec<&str> = attrs.iter().map(|(k, _)| k.as_str()).collect();
        let has_slug = attrs
            .iter()
            .any(|(k, v)| k == "subms.recipe.slug" && v == RECIPE_SLUG);
        let has_category = attrs
            .iter()
            .any(|(k, v)| k == "subms.recipe.category" && v == RECIPE_CATEGORY);
        let has_entries = keys.contains(&"subms.workload.entries");
        let has_host = keys.contains(&"subms.host");
        let has_tier = keys.contains(&"subms.hardware.tier");
        if has_slug && has_category && has_entries && has_host && has_tier {
            saw_full_attrs = true;
            break;
        }
    }
    assert!(
        saw_full_attrs,
        "on_summarize should attach the full inputs+meta attribute set",
    );
}

#[test]
fn per_record_emission_carries_hot_path_kind() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-example-kind-test");
    let observer = Arc::new(OtelObserver::new(meter));

    let h = SubMsPerfHarness::new("subms-otel-example", "rust").with_observer(observer);
    let _h = run_workload(
        h,
        WorkloadParams {
            entries: 50,
            capacity: 128,
            seed: 0xDEADC0DE,
        },
    );

    let (_count, attr_sets) = latency_count_and_attrs(&exporter, &provider);
    let mut hot_path_seen = false;
    let mut workload_seen = false;
    let mut lang_seen = false;
    for attrs in &attr_sets {
        for (k, v) in attrs {
            if k == "subms.stage.kind" && v == "hot_path" {
                hot_path_seen = true;
            }
            if k == "subms.workload" && v == "subms-otel-example" {
                workload_seen = true;
            }
            if k == "subms.lang" && v == "rust" {
                lang_seen = true;
            }
        }
    }
    assert!(
        hot_path_seen,
        "per-record attrs must include stage.kind = hot_path"
    );
    assert!(
        workload_seen,
        "per-record attrs must include subms.workload"
    );
    assert!(lang_seen, "per-record attrs must include subms.lang");
}
