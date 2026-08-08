//! Synchronous OpenTelemetry observer. Each [`SubMsObserver::on_record`]
//! call lands a `Histogram::record` on the configured [`Meter`]; the
//! [`SubMsObserver::on_summarize`] hook then re-emits the full bucket set
//! under the fuller attribute set drawn from the summary's inputs + meta.
//!
//! Overhead per record is one map lookup (stage -> Histogram cache) + the
//! OTEL `Histogram::record` cost. Fine for batch / one-shot stages; on the
//! hot path where sub-microsecond overhead matters, use
//! [`crate::OtelObserverAsync`].

use std::sync::{Arc, Mutex};

use opentelemetry::metrics::{Counter, Histogram, Meter};

use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver, SubMsStageKind};

use crate::bridge::{attributes_from_ctx, attributes_from_summary, histogram_for_stage};
use crate::drift::ReferenceDivergenceRecorder;
#[cfg(feature = "exemplars")]
use crate::exemplars::ExemplarReservoir;
use crate::{EXEMPLARS_KEPT_COUNTER_NAME, OPS_TOTAL_COUNTER_NAME};

/// Sync observer. Holds a [`Meter`] + a per-stage-name histogram cache (the
/// first time a stage name is seen, we build the histogram with kind-aware
/// boundaries and stash it).
///
/// **Attribute caveat**: `on_record` runs without access to the harness's
/// `inputs` / `meta`, so the hot-path attribute set is just
/// `workload / lang / stage / stage.kind`. The post-bench `on_summarize`
/// re-emits the headline percentiles plus the downsampled samples under the
/// fuller attribute set drawn from `summary.inputs` + `summary.meta` (see
/// [`crate::attributes_from_summary`]).
pub struct OtelObserver {
    meter: Meter,
    cache: Mutex<Vec<CachedStage>>,
    ops_counter: Counter<u64>,
    exemplars_kept: Counter<u64>,
    drift: ReferenceDivergenceRecorder,
    #[cfg(feature = "exemplars")]
    exemplars: Option<Arc<ExemplarReservoir>>,
}

struct CachedStage {
    name: String,
    kind: SubMsStageKind,
    histogram: Histogram<f64>,
}

impl OtelObserver {
    /// Build an observer bound to the given meter.
    pub fn new(meter: Meter) -> Self {
        let ops_counter = meter
            .u64_counter(OPS_TOTAL_COUNTER_NAME)
            .with_description("Per-stage operation counter; one increment per on_record call")
            .build();
        let exemplars_kept = meter
            .u64_counter(EXEMPLARS_KEPT_COUNTER_NAME)
            .with_description("Count of samples retained by an attached exemplar reservoir")
            .build();
        let drift = ReferenceDivergenceRecorder::new(meter.clone());
        Self {
            meter,
            cache: Mutex::new(Vec::new()),
            ops_counter,
            exemplars_kept,
            drift,
            #[cfg(feature = "exemplars")]
            exemplars: None,
        }
    }

    /// Attach an exemplar reservoir. Every recorded sample will also be
    /// offered to the reservoir; retained samples increment
    /// `subms.otel.exemplars_kept_total`. Call
    /// [`crate::ExemplarReservoir::publish`] yourself to push the gauge.
    #[cfg(feature = "exemplars")]
    pub fn with_exemplar_reservoir(mut self, reservoir: Arc<ExemplarReservoir>) -> Self {
        self.exemplars = Some(reservoir);
        self
    }

    /// Reference to the attached exemplar reservoir, if any. Returned so
    /// callers can publish + introspect from their own emit loop.
    #[cfg(feature = "exemplars")]
    pub fn exemplar_reservoir(&self) -> Option<Arc<ExemplarReservoir>> {
        self.exemplars.clone()
    }

    /// Record a reference-impl divergence. See
    /// [`ReferenceDivergenceRecorder`] for shape.
    pub fn record_reference_divergence(
        &self,
        stage: &str,
        kind: SubMsStageKind,
        reference_kind: &str,
        expected: &str,
        observed: &str,
    ) {
        self.drift
            .record(stage, kind, reference_kind, expected, observed);
    }

    #[cfg(feature = "exemplars")]
    fn offer_exemplar(&self, ctx: &ObservationCtx<'_>, ns: u64, attrs: &[opentelemetry::KeyValue]) {
        let Some(reservoir) = self.exemplars.as_ref() else {
            return;
        };
        if reservoir.offer(ctx, ns) {
            self.exemplars_kept.add(1, attrs);
        }
    }

    fn histogram_for(&self, stage: &str, kind: SubMsStageKind) -> Histogram<f64> {
        let mut cache = self.cache.lock().expect("observer cache poisoned");
        if let Some(entry) = cache.iter().find(|c| c.name == stage && c.kind == kind) {
            return entry.histogram.clone();
        }
        let histogram = histogram_for_stage(&self.meter, kind);
        cache.push(CachedStage {
            name: stage.to_string(),
            kind,
            histogram: histogram.clone(),
        });
        histogram
    }
}

impl SubMsObserver for OtelObserver {
    fn on_record(&self, ctx: &ObservationCtx, ns: u64) {
        let histogram = self.histogram_for(ctx.stage, ctx.stage_kind);
        let attrs = attributes_from_ctx(ctx);
        histogram.record(ns as f64 / 1e9, &attrs);
        self.ops_counter.add(1, &attrs);

        #[cfg(feature = "exemplars")]
        {
            self.offer_exemplar(ctx, ns, &attrs);
        }
    }

    fn on_summarize(&self, summary: &SubMsBenchSummary) {
        for stage in &summary.stages {
            let kind = {
                let cache = self.cache.lock().expect("observer cache poisoned");
                cache
                    .iter()
                    .find(|c| c.name == stage.name)
                    .map(|c| c.kind)
                    .unwrap_or(SubMsStageKind::Unspecified)
            };
            let histogram = self.histogram_for(&stage.name, kind);
            let attrs = attributes_from_summary(summary, stage, kind);
            histogram.record(stage.p50_ns as f64 / 1e9, &attrs);
            histogram.record(stage.p99_ns as f64 / 1e9, &attrs);
            histogram.record(stage.p999_ns as f64 / 1e9, &attrs);
            histogram.record(stage.max_ns as f64 / 1e9, &attrs);
            histogram.record(stage.mean_ns as f64 / 1e9, &attrs);
            if let Some(samples) = &stage.samples_ns {
                for ns in samples {
                    histogram.record(*ns as f64 / 1e9, &attrs);
                }
            }
        }
        #[cfg(feature = "exemplars")]
        if let Some(reservoir) = &self.exemplars {
            reservoir.publish(&self.meter);
        }
    }
}

#[cfg(test)]
#[path = "observer_tests.rs"]
mod observer_tests;
