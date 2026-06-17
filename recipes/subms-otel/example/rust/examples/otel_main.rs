//! End-to-end driver for the `subms-otel` recipe example. Runs the toy
//! workload twice - once under the synchronous [`OtelObserver`], once under
//! the asynchronous [`OtelObserverAsync`] - and prints the captured
//! OpenTelemetry signal so the demo is self-contained.
//!
//! ```sh
//! cargo run --release --example otel_main
//! ```
//!
//! There is no Jaeger / Prometheus / OTLP collector in the loop. The example
//! uses [`InMemoryMetricExporter`] (the SDK's testing exporter) as a stand-in
//! for "stdout exporter" and walks the captured `ResourceMetrics` itself.
//! In a real downstream wiring you would swap the exporter for OTLP /
//! Prometheus / whatever your backend speaks; the observer code does not
//! change.
//!
//! The headline: the workload (`run_workload`) contains zero OTel calls.
//! Registering one observer is all it takes for every [`SubMsPerfHarness`]
//! stage to emit a histogram point per recorded sample.

use std::sync::Arc;
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::{
    InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
    data::{AggregatedMetrics, MetricData},
};

use subms::{SubMsPerfHarness, print_summary, summarize};
use subms_otel_example::{OtelObserver, OtelObserverAsync, WorkloadParams, run_workload};

fn main() {
    println!("=== subms-otel recipe example: sync OtelObserver ===");
    run_sync();

    println!();
    println!("=== subms-otel recipe example: async OtelObserverAsync ===");
    run_async();
}

fn run_sync() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-example/sync");
    let observer = Arc::new(OtelObserver::new(meter));

    // The workload has no idea OTEL exists. The observer does it all.
    let h = SubMsPerfHarness::new("subms-otel-example", "rust").with_observer(observer);
    let h = run_workload(h, WorkloadParams::small());

    // Compute the summary; this fires on_summarize, which re-emits the
    // p50/p99/p999/max/mean (plus the downsampled timeline) under the fuller
    // attribute set drawn from inputs + meta.
    let summary = summarize(&h);
    print_summary(&summary, &mut std::io::stdout()).expect("print summary");

    provider.force_flush().expect("flush");
    println!();
    println!("-- captured OTEL signal (sync) --");
    dump_metrics(&exporter);

    // Provider shutdown closes the periodic reader and stops the background
    // export task cleanly.
    let _ = provider.shutdown();
}

fn run_async() {
    let (exporter, provider) = build_provider();
    let meter = provider.meter("subms-otel-example/async");
    let observer = Arc::new(
        OtelObserverAsync::builder(meter)
            .with_capacity(65_536)
            .with_drain_interval(Duration::from_millis(100))
            .build(),
    );

    let h = SubMsPerfHarness::new("subms-otel-example", "rust").with_observer(observer.clone());
    let h = run_workload(h, WorkloadParams::small());

    let summary = summarize(&h);
    print_summary(&summary, &mut std::io::stdout()).expect("print summary");

    // Force the async observer to drain everything it has queued before we
    // flush the provider. Without this the periodic reader might race the
    // final batch of samples.
    observer.flush();
    provider.force_flush().expect("flush");

    println!();
    println!("-- captured OTEL signal (async) --");
    println!(
        "  dropped samples (back-pressure): {}",
        observer.dropped_count()
    );
    dump_metrics(&exporter);

    let _ = provider.shutdown();
}

fn build_provider() -> (InMemoryMetricExporter, SdkMeterProvider) {
    let exporter = InMemoryMetricExporter::default();
    let reader = PeriodicReader::builder(exporter.clone())
        .with_interval(Duration::from_secs(60))
        .build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    (exporter, provider)
}

/// Walk the captured `ResourceMetrics` and print a flat human-readable
/// rollup. A real backend reads the same fields via OTLP / Prometheus
/// scraping; we just dump them so the example can be run without one.
fn dump_metrics(exporter: &InMemoryMetricExporter) {
    let snapshots = exporter.get_finished_metrics().expect("get metrics");
    let Some(rm) = snapshots.last() else {
        println!("  (no metrics captured - try `cargo run --release`)");
        return;
    };
    for sm in rm.scope_metrics() {
        for m in sm.metrics() {
            match m.data() {
                AggregatedMetrics::F64(MetricData::Histogram(h)) => {
                    for dp in h.data_points() {
                        let stage = attr(dp.attributes(), "subms.stage");
                        let kind = attr(dp.attributes(), "subms.stage.kind");
                        let slug = attr(dp.attributes(), "subms.recipe.slug");
                        println!(
                            "  {name:<24} stage={stage:<10} kind={kind:<10} slug={slug:<22} \
                             count={count:>6} sum={sum:.6}s min={min:?} max={max:?}",
                            name = m.name(),
                            stage = stage.unwrap_or("-".into()),
                            kind = kind.unwrap_or("-".into()),
                            slug = slug.unwrap_or("-".into()),
                            count = dp.count(),
                            sum = dp.sum(),
                            min = dp.min(),
                            max = dp.max(),
                        );
                    }
                }
                AggregatedMetrics::U64(MetricData::Sum(s)) => {
                    for dp in s.data_points() {
                        println!(
                            "  {name:<24} value={value}",
                            name = m.name(),
                            value = dp.value()
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn attr<'a, I>(attrs: I, key: &str) -> Option<String>
where
    I: Iterator<Item = &'a opentelemetry::KeyValue>,
{
    for kv in attrs {
        if kv.key.as_str() == key {
            return Some(kv.value.to_string());
        }
    }
    None
}
