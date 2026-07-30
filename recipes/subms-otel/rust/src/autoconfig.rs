//! Env-driven one-line bootstrap. Reads the standard `OTEL_*` and `SUBMS_*`
//! variables, picks an exporter, builds the resource set, and wires an
//! [`OtelObserver`] / [`OtelObserverAsync`] against the resulting `Meter`.
//!
//! Runtime observer registry: consumers call [`register_observer`] at startup;
//! [`registered_observers`] surfaces the list and [`auto_configure`] prefers
//! the first registered observer over a fresh build.

use std::env;
use std::sync::{Arc, Mutex, OnceLock};

use opentelemetry::global::BoxedTracer;
use opentelemetry::metrics::{Meter, MeterProvider};
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;

use subms::SubMsObserver;

use crate::resource::SubMsOtelResource;
use crate::{OtelObserver, OtelObserverAsync};

#[cfg(feature = "exporter-otlp")]
use crate::exporter_otlp::{ExporterOtlpHelper, OtlpProtocol};
#[cfg(feature = "exporter-prometheus")]
use crate::exporter_prometheus::ExporterPrometheusHelper;
#[cfg(feature = "exporter-stdout")]
use crate::exporter_stdout::ExporterStdoutHelper;

/// Configured providers + handles returned from [`auto_configure`]. The caller
/// registers `observer` on the harness via `with_observer` and is expected to
/// call `force_flush` / `shutdown` on the providers at program exit.
pub struct SubMsOtelAutoConfig {
    pub meter: Meter,
    pub tracer: BoxedTracer,
    pub observer: Arc<dyn SubMsObserver>,
    pub meter_provider: SdkMeterProvider,
    pub tracer_provider: SdkTracerProvider,
}

/// One entry in the runtime auto-registration list. Consumers call
/// [`register_observer`] at startup; `auto_configure` uses the first
/// registered observer rather than constructing a fresh one.
pub struct SubMsObserverRegistration {
    pub name: &'static str,
    pub observer: Arc<dyn SubMsObserver>,
}

fn registry() -> &'static Mutex<Vec<SubMsObserverRegistration>> {
    static REGISTERED: OnceLock<Mutex<Vec<SubMsObserverRegistration>>> = OnceLock::new();
    REGISTERED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register an observer at startup. `auto_configure` picks the first
/// registered observer ahead of constructing a fresh one.
pub fn register_observer(name: &'static str, observer: Arc<dyn SubMsObserver>) {
    let mut lock = registry().lock().expect("observer registry poisoned");
    lock.push(SubMsObserverRegistration { name, observer });
}

/// Drop every registered observer. Mostly for tests.
pub fn clear_registered_observers() {
    let mut lock = registry().lock().expect("observer registry poisoned");
    lock.clear();
}

/// Snapshot the registry as a list of observers.
pub fn registered_observers() -> Vec<Arc<dyn SubMsObserver>> {
    let lock = registry().lock().expect("observer registry poisoned");
    lock.iter().map(|r| Arc::clone(&r.observer)).collect()
}

/// Read env, build providers + observer, return everything wired together.
pub fn auto_configure() -> SubMsOtelAutoConfig {
    let resource = Resource::builder_empty()
        .with_attributes(SubMsOtelResource::detect())
        .build();
    let (meter_provider, tracer_provider) = build_providers(resource);

    let meter = meter_provider.meter("subms-otel");
    let tracer = tracer_provider.tracer("subms-otel");
    let boxed_tracer = BoxedTracer::new(Box::new(tracer));

    let observer: Arc<dyn SubMsObserver> =
        if let Some(o) = registered_observers().into_iter().next() {
            o
        } else if pick_async() {
            Arc::new(OtelObserverAsync::new(meter.clone()))
        } else {
            Arc::new(OtelObserver::new(meter.clone()))
        };

    SubMsOtelAutoConfig {
        meter,
        tracer: boxed_tracer,
        observer,
        meter_provider,
        tracer_provider,
    }
}

fn pick_async() -> bool {
    !matches!(
        env::var("SUBMS_OTEL_ASYNC").ok().as_deref(),
        Some("false") | Some("0") | Some("no")
    )
}

#[allow(dead_code)]
fn exemplar_k() -> usize {
    env::var("SUBMS_OTEL_EXEMPLARS_K")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(crate::DEFAULT_RESERVOIR_K)
}

fn build_providers(resource: Resource) -> (SdkMeterProvider, SdkTracerProvider) {
    let endpoint_raw = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|s| !s.is_empty());
    let _has_endpoint = endpoint_raw.is_some();
    #[cfg(feature = "exporter-otlp")]
    let endpoint = endpoint_raw;
    #[cfg(not(feature = "exporter-otlp"))]
    let _ = endpoint_raw;

    #[cfg(feature = "exporter-otlp")]
    if endpoint.is_some() {
        let protocol =
            OtlpProtocol::from_env(env::var("OTEL_EXPORTER_OTLP_PROTOCOL").ok().as_deref());
        if let Some(pair) =
            ExporterOtlpHelper::build(endpoint.as_deref(), protocol, resource.clone())
        {
            return pair;
        }
    }

    #[cfg(feature = "exporter-prometheus")]
    {
        let prom_requested = env::var("SUBMS_OTEL_PROMETHEUS")
            .map(|v| matches!(v.as_str(), "1" | "true"))
            .unwrap_or(false);
        if !_has_endpoint
            && prom_requested
            && let Some((mp, _exporter)) = ExporterPrometheusHelper::build(resource.clone())
        {
            let tp = SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .build();
            return (mp, tp);
        }
    }

    #[cfg(feature = "exporter-stdout")]
    {
        return ExporterStdoutHelper::build(resource);
    }

    #[allow(unreachable_code)]
    no_op_providers(resource)
}

#[allow(dead_code)]
fn no_op_providers(resource: Resource) -> (SdkMeterProvider, SdkTracerProvider) {
    let mp = SdkMeterProvider::builder()
        .with_resource(resource.clone())
        .build();
    let tp = SdkTracerProvider::builder().with_resource(resource).build();
    (mp, tp)
}

#[cfg(test)]
#[path = "autoconfig_tests.rs"]
mod autoconfig_tests;
