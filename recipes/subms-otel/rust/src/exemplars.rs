//! Per-bucket exemplar reservoir. For every `(stage, bucket)` pair the
//! reservoir keeps the slowest K samples seen, each with the full attribute
//! context that was active when the sample landed.
//!
//! The reservoir is wired into [`crate::OtelObserver`] (and the async
//! sibling) via the `with_exemplar_reservoir` builder method. When the
//! observer attaches the reservoir, every recorded sample is offered to it;
//! if the sample is slower than the current floor of its bucket's reservoir,
//! it evicts the fastest entry and lands in its place.
//!
//! Reservoir contents are published every drain pass as a synthetic gauge
//! named [`EXEMPLAR_GAUGE_NAME`] - one data point per kept sample, with the
//! bucket boundary as a label. Consumers reading the gauge can plot
//! per-bucket slow traces directly.

use std::collections::HashMap;
use std::sync::Mutex;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Gauge, Meter};

use subms::{ObservationCtx, SubMsStageKind};

use crate::bridge::{attributes_from_ctx, histogram_boundaries};

/// Default reservoir size per `(stage, bucket)` pair.
pub const DEFAULT_RESERVOIR_K: usize = 5;

/// Gauge name used to publish exemplar samples.
pub const EXEMPLAR_GAUGE_NAME: &str = "subms.exemplars";

/// One kept sample: the raw nanosecond timing plus the attribute set that
/// was active at record time.
#[derive(Clone, Debug)]
pub struct Exemplar {
    pub stage: String,
    pub kind: SubMsStageKind,
    pub bucket_upper_seconds: f64,
    pub ns: u64,
    pub attributes: Vec<KeyValue>,
}

/// Per-bucket reservoir of the slowest K samples seen. Thread-safe; designed
/// to be wrapped in `Arc` and shared across observer instances.
pub struct ExemplarReservoir {
    k: usize,
    // Keyed by (stage_name, bucket_idx). Vec<Exemplar> is kept sorted ascending
    // by ns; insertion picks the slowest by evicting position 0.
    buckets: Mutex<HashMap<(String, usize), Vec<Exemplar>>>,
}

impl Default for ExemplarReservoir {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_RESERVOIR_K)
    }
}

impl ExemplarReservoir {
    /// Build with the default per-bucket K.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build with a custom per-bucket K. Panics if K is 0.
    pub fn with_capacity(k: usize) -> Self {
        assert!(k > 0, "reservoir K must be > 0");
        Self {
            k,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Per-bucket K used by this reservoir.
    pub fn capacity(&self) -> usize {
        self.k
    }

    /// Offer a recorded sample. Returns `true` if it was retained (and
    /// optionally displaced an existing entry).
    pub fn offer(&self, ctx: &ObservationCtx<'_>, ns: u64) -> bool {
        let bucket_idx = bucket_index_for(ctx.stage_kind, ns);
        let bucket_upper = bucket_upper_seconds(ctx.stage_kind, bucket_idx);

        let mut map = self.buckets.lock().expect("exemplar reservoir poisoned");
        let key = (ctx.stage.to_string(), bucket_idx);
        let entries = map.entry(key).or_default();

        if entries.len() < self.k {
            entries.push(Exemplar {
                stage: ctx.stage.to_string(),
                kind: ctx.stage_kind,
                bucket_upper_seconds: bucket_upper,
                ns,
                attributes: attributes_from_ctx(ctx),
            });
            entries.sort_by_key(|e| e.ns);
            return true;
        }
        // Reservoir full: evict the fastest if the new sample beats it.
        if ns > entries[0].ns {
            entries[0] = Exemplar {
                stage: ctx.stage.to_string(),
                kind: ctx.stage_kind,
                bucket_upper_seconds: bucket_upper,
                ns,
                attributes: attributes_from_ctx(ctx),
            };
            entries.sort_by_key(|e| e.ns);
            return true;
        }
        false
    }

    /// Snapshot the current reservoir contents. Order is not guaranteed
    /// across (stage, bucket) tuples, but each tuple's entries are sorted
    /// ascending by ns.
    pub fn snapshot(&self) -> Vec<Exemplar> {
        let map = self.buckets.lock().expect("exemplar reservoir poisoned");
        let mut out = Vec::new();
        for entries in map.values() {
            out.extend(entries.iter().cloned());
        }
        out
    }

    /// Discard everything.
    pub fn clear(&self) {
        self.buckets
            .lock()
            .expect("exemplar reservoir poisoned")
            .clear();
    }

    /// Publish every kept sample as a single point on the `subms.exemplars`
    /// gauge. Each point carries the bucket upper-bound + the original
    /// attribute set + the ns value as `subms.exemplar.ns`. Called from the
    /// host observer's drain / flush path.
    pub fn publish(&self, meter: &Meter) {
        let gauge: Gauge<u64> = meter
            .u64_gauge(EXEMPLAR_GAUGE_NAME)
            .with_description("Slow-sample exemplars retained per (stage, bucket).")
            .build();
        for ex in self.snapshot() {
            let mut attrs = ex.attributes.clone();
            attrs.push(KeyValue::new(
                "subms.exemplar.bucket_upper_s",
                ex.bucket_upper_seconds,
            ));
            attrs.push(KeyValue::new("subms.exemplar.ns", ex.ns as i64));
            gauge.record(ex.ns, &attrs);
        }
    }
}

/// Find the bucket index for `ns` given the kind's bucket schedule. Returns
/// the index into `histogram_boundaries(kind)`; values past the last
/// boundary land in the implicit overflow bucket.
fn bucket_index_for(kind: SubMsStageKind, ns: u64) -> usize {
    let secs = ns as f64 / 1e9;
    let bounds = histogram_boundaries(kind);
    for (idx, bound) in bounds.iter().enumerate() {
        if secs <= *bound {
            return idx;
        }
    }
    bounds.len()
}

/// Upper bound (seconds) for the given bucket index. The overflow bucket
/// reports `f64::INFINITY`.
fn bucket_upper_seconds(kind: SubMsStageKind, idx: usize) -> f64 {
    let bounds = histogram_boundaries(kind);
    if idx < bounds.len() {
        bounds[idx]
    } else {
        f64::INFINITY
    }
}

#[cfg(test)]
#[path = "exemplars_unit_tests.rs"]
mod unit_tests;

#[cfg(all(test, feature = "observer"))]
#[path = "exemplars_tests.rs"]
mod integration_tests;
