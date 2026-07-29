//! Span-emitting observer. On every [`SubMsObserver::on_record`] this fires a
//! tiny span whose start time is reconstructed as `now - ns`; if the calling
//! thread already has an active OTEL Context with a parent span, that parent
//! is inherited so a higher-level trace (e.g. a Spring HTTP filter) can flow
//! through the bench instrumentation.

use std::time::SystemTime;

use opentelemetry::Context;
use opentelemetry::trace::{Span, SpanBuilder, Tracer};

use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver};

use crate::bridge::attributes_from_ctx;

/// Span name used for every emitted record. Stage identity is on the attrs.
pub const TRACING_SPAN_NAME: &str = "subms.stage.record";

/// Span-emitting observer. Holds a tracer + the span name template.
pub struct TracingObserver<T: Tracer + Send + Sync> {
    tracer: T,
}

impl<T> TracingObserver<T>
where
    T: Tracer + Send + Sync,
    T::Span: Span + Send + Sync + 'static,
{
    /// Build bound to the given tracer.
    pub fn new(tracer: T) -> Self {
        Self { tracer }
    }
}

impl<T> SubMsObserver for TracingObserver<T>
where
    T: Tracer + Send + Sync,
    T::Span: Span + Send + Sync + 'static,
{
    fn on_record(&self, ctx: &ObservationCtx, ns: u64) {
        let now = SystemTime::now();
        let start = now
            .checked_sub(std::time::Duration::from_nanos(ns))
            .unwrap_or(now);
        let parent_ctx = Context::current();
        let attrs = attributes_from_ctx(ctx);
        let mut span = SpanBuilder::from_name(TRACING_SPAN_NAME)
            .with_start_time(start)
            .with_attributes(attrs)
            .start_with_context(&self.tracer, &parent_ctx);
        span.end_with_timestamp(now);
    }

    fn on_summarize(&self, _summary: &SubMsBenchSummary) {
        // No span on summarize - the bridge's export_timer / export_summary
        // already covers the post-bench narrative.
    }
}

#[cfg(test)]
#[path = "tracing_observer_tests.rs"]
mod tracing_observer_tests;
