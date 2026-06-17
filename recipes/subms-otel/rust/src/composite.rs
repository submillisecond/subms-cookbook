//! Fan-out observer: wire two or more [`SubMsObserver`]s up to a single
//! harness without modifying the harness API.

use std::sync::Arc;

use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver};

/// Holds a list of inner observers and forwards every event to each. Lets a
/// consumer combine, e.g., an [`crate::OtelObserver`] with a custom
/// Prometheus impl and a logging observer at once.
///
/// ```ignore
/// use std::sync::Arc;
/// use subms_otel::{CompositeObserver, OtelObserver};
///
/// let composite = CompositeObserver::new(vec![
///     Arc::new(OtelObserver::new(meter)) as Arc<dyn subms::SubMsObserver>,
///     Arc::new(my_prom_observer),
/// ]);
/// let h = SubMsPerfHarness::new("workload", "rust").with_observer(Arc::new(composite));
/// ```
pub struct CompositeObserver {
    inner: Vec<Arc<dyn SubMsObserver>>,
}

impl CompositeObserver {
    /// Build from a ready-made vec of observers.
    pub fn new(inner: Vec<Arc<dyn SubMsObserver>>) -> Self {
        Self { inner }
    }

    /// Append another observer to the fan-out list. Chainable.
    pub fn with(mut self, observer: Arc<dyn SubMsObserver>) -> Self {
        self.inner.push(observer);
        self
    }

    /// Read the inner observer list. Mostly for tests / introspection.
    pub fn observers(&self) -> &[Arc<dyn SubMsObserver>] {
        &self.inner
    }
}

impl SubMsObserver for CompositeObserver {
    fn on_record(&self, ctx: &ObservationCtx, ns: u64) {
        for obs in &self.inner {
            obs.on_record(ctx, ns);
        }
    }

    fn on_summarize(&self, summary: &SubMsBenchSummary) {
        for obs in &self.inner {
            obs.on_summarize(summary);
        }
    }
}
