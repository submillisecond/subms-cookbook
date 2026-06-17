//! Always-available one-shot bridge from a typed [`SubMsBenchSummary`] /
//! [`SubMsTimer`] to OpenTelemetry instruments. No background threads, no
//! per-sample channel - just walks the summary and emits.

use std::collections::BTreeMap;
use std::time::SystemTime;

use opentelemetry::KeyValue;
use opentelemetry::metrics::Meter;
use opentelemetry::trace::{Span, SpanBuilder, Tracer};

use subms::{ObservationCtx, SubMsBenchSummary, SubMsStageKind, SubMsStageSummary, SubMsTimer};

/// Canonical name for the per-stage latency histogram. Consumers filter on
/// this in Grafana / dashboards.
pub const HISTOGRAM_NAME: &str = "subms.latency";

/// Canonical unit for the latency histogram. Seconds is OTEL convention for
/// duration measurements - we emit ns / 1e9 to satisfy it.
pub const HISTOGRAM_UNIT: &str = "s";

/// Histogram bucket boundaries (in seconds) keyed off the stage kind. Adapter
/// code applies these when first constructing the `Histogram` instrument for
/// a given stage kind.
///
/// - `HotPath` covers 50ns .. 1ms, the sub-ms p99 budget recipes target.
/// - `BatchOp` / `OneShot` cover 1us .. 1s, where whole-structure ops live.
/// - `Unspecified` returns an empty slice so OTEL's default boundaries apply.
pub fn histogram_boundaries(kind: SubMsStageKind) -> &'static [f64] {
    const HOT: &[f64] = &[
        5e-8, 1e-7, 2e-7, 5e-7, 1e-6, 2e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3,
    ];
    const BATCH: &[f64] = &[1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1.0];
    match kind {
        SubMsStageKind::HotPath => HOT,
        SubMsStageKind::BatchOp | SubMsStageKind::OneShot => BATCH,
        SubMsStageKind::Unspecified => &[],
        _ => &[],
    }
}

/// Build the OTEL attribute set for a hot-path record. Just the harness
/// identity the [`ObservationCtx`] carries - inputs and meta arrive later via
/// the summary.
pub fn attributes_from_ctx(ctx: &ObservationCtx<'_>) -> Vec<KeyValue> {
    vec![
        KeyValue::new("subms.workload", ctx.workload.to_string()),
        KeyValue::new("subms.lang", ctx.lang.to_string()),
        KeyValue::new("subms.stage", ctx.stage.to_string()),
        KeyValue::new("subms.stage.kind", ctx.stage_kind.as_str()),
    ]
}

/// Build the full attribute set for a stage when we have the post-bench
/// summary. Pulls workload + lang + stage from the summary and folds in any
/// of the canonical inputs/meta keys that are present.
pub fn attributes_from_summary(
    summary: &SubMsBenchSummary,
    stage: &SubMsStageSummary,
    kind: SubMsStageKind,
) -> Vec<KeyValue> {
    let mut attrs = vec![
        KeyValue::new("subms.workload", summary.workload.clone()),
        KeyValue::new("subms.lang", summary.lang.clone()),
        KeyValue::new("subms.stage", stage.name.clone()),
        KeyValue::new("subms.stage.kind", kind.as_str()),
    ];
    push_meta_attrs(&mut attrs, &summary.inputs, &summary.meta);
    attrs
}

/// Fold inputs + meta entries into an attribute set. Public so the async
/// observer can use the same logic when draining cached summary state.
pub(crate) fn push_meta_attrs(
    attrs: &mut Vec<KeyValue>,
    inputs: &BTreeMap<String, String>,
    meta: &BTreeMap<String, String>,
) {
    fn put(
        attrs: &mut Vec<KeyValue>,
        map: &BTreeMap<String, String>,
        src: &str,
        dst: &'static str,
    ) {
        if let Some(v) = map.get(src) {
            attrs.push(KeyValue::new(dst, v.clone()));
        }
    }
    put(attrs, meta, "subms.recipe.slug", "subms.recipe.slug");
    put(
        attrs,
        meta,
        "subms.recipe.category",
        "subms.recipe.category",
    );
    put(
        attrs,
        meta,
        "subms.workload.feature",
        "subms.workload.feature",
    );
    put(attrs, inputs, "entries", "subms.workload.entries");
    put(attrs, inputs, "seed", "subms.workload.seed");
    put(attrs, meta, "host", "subms.host");
    put(attrs, meta, "hardware_tier", "subms.hardware.tier");
    put(attrs, meta, "crate_version", "subms.crate.version");
}

/// Export every stage of a [`SubMsBenchSummary`] as a Histogram emission on
/// the given [`Meter`]. Each percentile (p50, p99, p999, max, mean, stddev)
/// and the downsampled chronological samples are recorded as histogram
/// values under the stage's full attribute set.
///
/// The histogram name is [`HISTOGRAM_NAME`] (one instrument per
/// (workload, stage) - the attribute set carries the stage identity).
///
/// Stage kind is not carried on the summary; this function defaults to
/// [`SubMsStageKind::Unspecified`] for boundary selection. Callers that know
/// the per-stage kind ahead of time should use [`OtelObserver`] / the
/// kind-tracking observer surface instead.
pub fn export_summary(summary: &SubMsBenchSummary, meter: &Meter) {
    for stage in &summary.stages {
        let kind = SubMsStageKind::Unspecified;
        let attrs = attributes_from_summary(summary, stage, kind);
        let histogram = histogram_for_stage(meter, kind);
        // Emit the headline percentiles as discrete records. This is the
        // "summary mode" - downstream systems can read p50/p99/etc. straight
        // off the histogram buckets that contain these values.
        record_seconds(&histogram, stage.p50_ns, &attrs);
        record_seconds(&histogram, stage.p99_ns, &attrs);
        record_seconds(&histogram, stage.p999_ns, &attrs);
        record_seconds(&histogram, stage.max_ns, &attrs);
        record_seconds(&histogram, stage.mean_ns, &attrs);
        // Replay the downsampled timeline so backends with native histogram
        // aggregation see the actual distribution shape.
        if let Some(samples) = &stage.samples_ns {
            for ns in samples {
                record_seconds(&histogram, *ns, &attrs);
            }
        }
    }
}

fn record_seconds(histogram: &opentelemetry::metrics::Histogram<f64>, ns: u64, attrs: &[KeyValue]) {
    histogram.record(ns as f64 / 1e9, attrs);
}

/// Build a `Histogram<f64>` whose bucket boundaries match the given stage
/// kind. Public-ish helper used by both [`export_summary`] and the observer
/// modules.
pub(crate) fn histogram_for_stage(
    meter: &Meter,
    kind: SubMsStageKind,
) -> opentelemetry::metrics::Histogram<f64> {
    let bounds = histogram_boundaries(kind);
    let mut builder = meter
        .f64_histogram(HISTOGRAM_NAME)
        .with_unit(HISTOGRAM_UNIT)
        .with_description("subms per-stage latency histogram");
    if !bounds.is_empty() {
        builder = builder.with_boundaries(bounds.to_vec());
    }
    builder.build()
}

/// Export a [`SubMsTimer`] timeline as a parent span + one child span per
/// checkpoint. Span start times are reconstructed as `now - elapsed_ns` so
/// the timing offsets the timer captured are preserved end-to-end.
///
/// The parent span's name is the timer's name (or `"subms.timer"` for an
/// unnamed timer). Each checkpoint becomes a child span whose duration is
/// the checkpoint's `since_last_ns`.
pub fn export_timer<T: Tracer>(timer: &SubMsTimer, tracer: &T)
where
    T::Span: Span + Send + Sync + 'static,
{
    let total_ns = timer.elapsed_ns();
    let now = SystemTime::now();
    let parent_start = now
        .checked_sub(std::time::Duration::from_nanos(total_ns))
        .unwrap_or(now);

    let parent_name: std::borrow::Cow<'static, str> = if timer.name().is_empty() {
        "subms.timer".into()
    } else {
        timer.name().to_string().into()
    };
    let mut parent = SpanBuilder::from_name(parent_name)
        .with_start_time(parent_start)
        .with_attributes(vec![
            KeyValue::new("subms.timer.total_ns", total_ns as i64),
            KeyValue::new("subms.timer.checkpoints", timer.checkpoints().len() as i64),
        ])
        .start(tracer);

    for cp in timer.checkpoints() {
        let child_start = parent_start
            .checked_add(std::time::Duration::from_nanos(
                cp.since_start_ns.saturating_sub(cp.since_last_ns),
            ))
            .unwrap_or(parent_start);
        let child_end = parent_start
            .checked_add(std::time::Duration::from_nanos(cp.since_start_ns))
            .unwrap_or(parent_start);

        let mut child = SpanBuilder::from_name(cp.label.clone())
            .with_start_time(child_start)
            .with_attributes(vec![
                KeyValue::new("subms.timer.since_last_ns", cp.since_last_ns as i64),
                KeyValue::new("subms.timer.since_start_ns", cp.since_start_ns as i64),
                KeyValue::new("subms.timer.is_stop", cp.is_stop),
            ])
            .start(tracer);
        child.end_with_timestamp(child_end);
    }

    parent.end_with_timestamp(now);
}
