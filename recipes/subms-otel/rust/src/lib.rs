//! `subms-otel` - OpenTelemetry bridge for the [`subms`] perf harness.
//!
//! Wires the harness's [`SubMsObserver`] hook + the post-bench
//! [`SubMsBenchSummary`] + the [`SubMsTimer`] timeline into OpenTelemetry
//! Histogram / Span emission, without the harness itself ever pulling in
//! the OTEL dependency tree.
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use subms_events::{Event, EventDispatcher, EventLevel, listener};
//! use subms_otel::OtelEventBridge;
//!
//! // A trading gateway's health flips + lifecycle events flow over a
//! // subms-events bus. Attaching OtelEventBridge forwards each one to
//! // OpenTelemetry as the `subms.events.total` counter - a no-op until a
//! // MeterProvider is installed, so it is always safe to wire in.
//! let seen = Arc::new(Mutex::new(Vec::new()));
//! let tap = Arc::clone(&seen);
//!
//! let mut bus = EventDispatcher::sync();
//! bus.add_bridge(Arc::new(OtelEventBridge::new()));
//! bus.add_listener(listener(move |e: &Event| tap.lock().unwrap().push(e.topic.clone())));
//!
//! bus.emit(Event::transition("gateway.health", EventLevel::Warn, "gateway", "UP", "DEGRADED"));
//! assert_eq!(seen.lock().unwrap().as_slice(), ["gateway.health"]);
//! ```
//!
//! # Feature flags
//!
//! Capability features:
//!
//! - `bridge` (default) - always-available bridge: post-hoc helpers,
//!   [`CompositeObserver`], counter + gauge surface constants, the
//!   reference-drift recorder.
//! - `observer` - both [`OtelObserver`] (sync) and [`OtelObserverAsync`]
//!   (hand-rolled SPSC ring; target overhead < 300 ns per `on_record`).
//! - `exemplars` - per-bucket tail-debug exemplar reservoir
//!   ([`ExemplarReservoir`]). Configurable K; default K = 5.
//! - `tracing` - span emission per stage record + W3C TraceContext parent
//!   inheritance ([`TracingObserver`]).
//!
//! Bootstrap:
//!
//! - `autoconfig` - env-driven one-line SDK wiring + runtime observer registry.
//!
//! Exporter helpers (independent; pick what you ship):
//!
//! - `exporter-otlp` - [`OtlpBuilder`] + [`ExporterOtlpHelper`].
//! - `exporter-prometheus` - [`PrometheusBuilder`] + [`ExporterPrometheusHelper`].
//! - `exporter-stdout` - [`ExporterStdoutHelper`].
//!
//! Both observers honour the same semantic-conventions table:
//!
//! | Attribute key             | Source                                                |
//! |---------------------------|-------------------------------------------------------|
//! | `subms.workload`          | `ctx.workload` / `summary.workload`                   |
//! | `subms.lang`              | `ctx.lang` / `summary.lang`                           |
//! | `subms.stage`             | `ctx.stage` / stage name                              |
//! | `subms.stage.kind`        | `ctx.stage_kind.as_str()`                             |
//! | `subms.recipe.slug`       | `meta["subms.recipe.slug"]`                           |
//! | `subms.recipe.category`   | `meta["subms.recipe.category"]`                       |
//! | `subms.workload.feature`  | `meta["subms.workload.feature"]`                      |
//! | `subms.workload.entries`  | `inputs["entries"]`                                   |
//! | `subms.workload.seed`     | `inputs["seed"]`                                      |
//! | `subms.host`              | `meta["host"]`                                        |
//! | `subms.hardware.tier`     | `meta["hardware_tier"]`                               |
//! | `subms.crate.version`     | `meta["crate_version"]`                               |
//!
//! Inputs and meta arrive only via `on_summarize` (the harness's
//! [`ObservationCtx`] is intentionally minimal). The sync observer therefore
//! emits with just `workload`, `lang`, `stage`, and `stage.kind` per record;
//! `on_summarize` re-emits the whole histogram bucket set under the fuller
//! attribute set. The async observer caches the latest summary's inputs+meta
//! between drain passes and applies them to drained samples once available.
//!
//! [`SubMsObserver`]: subms::SubMsObserver
//! [`SubMsBenchSummary`]: subms::SubMsBenchSummary
//! [`SubMsTimer`]: subms::SubMsTimer
//! [`ObservationCtx`]: subms::ObservationCtx
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-otel>

// Public re-exports of the harness types consumers will need.
pub use subms::{
    ObservationCtx, SubMsBenchSummary, SubMsObserver, SubMsStageKind, SubMsStageSummary,
    SubMsTimer, SubMsTimerCheckpoint,
};

#[cfg(feature = "autoconfig")]
mod autoconfig;
#[cfg(feature = "bridge")]
mod bridge;
#[cfg(feature = "bridge")]
mod composite;
#[cfg(feature = "bridge")]
mod drift;
#[cfg(feature = "exemplars")]
mod exemplars;
#[cfg(feature = "exporter-otlp")]
mod exporter_otlp;
#[cfg(feature = "exporter-prometheus")]
mod exporter_prometheus;
#[cfg(feature = "exporter-stdout")]
mod exporter_stdout;
#[cfg(feature = "observer")]
mod observer;
#[cfg(feature = "observer")]
mod observer_async;
#[cfg(feature = "bridge")]
mod resource;
#[cfg(feature = "bridge")]
mod state;
#[cfg(feature = "tracing")]
mod tracing_observer;

// Re-export the pinned opentelemetry so downstream recipes (e.g. subms-health)
// can name OTEL types at the exact version this bridge uses.
#[cfg(feature = "bridge")]
pub use opentelemetry;

#[cfg(feature = "autoconfig")]
pub use autoconfig::{
    SubMsObserverRegistration, SubMsOtelAutoConfig, auto_configure, clear_registered_observers,
    register_observer, registered_observers,
};
#[cfg(feature = "bridge")]
pub use bridge::{
    HISTOGRAM_NAME, HISTOGRAM_UNIT, attributes_from_ctx, attributes_from_summary, export_summary,
    export_timer, histogram_boundaries,
};
#[cfg(feature = "bridge")]
pub use composite::CompositeObserver;
#[cfg(feature = "bridge")]
pub use drift::{
    REFERENCE_DIVERGENCE_COUNTER_NAME, ReferenceDivergenceRecorder, divergence_attributes,
    record_reference_divergence,
};
#[cfg(feature = "exemplars")]
pub use exemplars::{DEFAULT_RESERVOIR_K, EXEMPLAR_GAUGE_NAME, Exemplar, ExemplarReservoir};
#[cfg(feature = "exporter-otlp")]
pub use exporter_otlp::{ExporterOtlpHelper, OtlpBuilder, OtlpProtocol};
#[cfg(feature = "exporter-prometheus")]
pub use exporter_prometheus::{
    ExporterPrometheusHelper, PrometheusBuilder, PrometheusTextExporter,
};
#[cfg(feature = "exporter-stdout")]
pub use exporter_stdout::ExporterStdoutHelper;
#[cfg(feature = "observer")]
pub use observer::OtelObserver;
#[cfg(feature = "observer")]
pub use observer_async::OtelObserverAsync;
#[cfg(feature = "bridge")]
pub use resource::SubMsOtelResource;
#[cfg(feature = "bridge")]
pub use state::{OtelEventBridge, StateTransitionRecorder};
#[cfg(feature = "tracing")]
pub use tracing_observer::{TRACING_SPAN_NAME, TracingObserver};

/// Counter name incremented on every `on_record` regardless of which observer
/// surface received the sample.
#[cfg(feature = "bridge")]
pub const OPS_TOTAL_COUNTER_NAME: &str = "subms.bench.ops_total";

/// ObservableGauge name published by [`OtelObserverAsync`] tracking the
/// current channel depth.
#[cfg(feature = "bridge")]
pub const IN_FLIGHT_GAUGE_NAME: &str = "subms.bench.in_flight";

/// Counter name incremented when a sample is retained by an
/// [`ExemplarReservoir`].
#[cfg(feature = "bridge")]
pub const EXEMPLARS_KEPT_COUNTER_NAME: &str = "subms.otel.exemplars_kept_total";

/// Counter name for back-pressure-dropped samples. Promoted from the async
/// observer's private constant so dashboards can wire it directly.
#[cfg(feature = "bridge")]
pub const DROPPED_TOTAL_COUNTER_NAME: &str = "subms.otel.dropped_total";

#[cfg(test)]
#[path = "test_common.rs"]
mod test_common;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, OnceLock};

    // Serializes tests that mutate process env / the global observer registry:
    // consolidated into one test binary they share a process, so they need one lock.
    pub fn global_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}

#[cfg(all(test, feature = "observer"))]
#[path = "counter_gauge_tests.rs"]
mod counter_gauge_tests;

#[cfg(all(test, feature = "autoconfig"))]
#[path = "inventory_register_tests.rs"]
mod inventory_register_tests;

#[cfg(all(test, feature = "bridge"))]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
