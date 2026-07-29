//! Generic state-transition counter. A reusable adapter primitive for any recipe
//! that emits "X moved from state A to state B" events - e.g. `subms-health`
//! status flips (UP -> DEGRADED -> UP). Mirrors the [`crate::ReferenceDivergenceRecorder`]
//! shape: hold one Counter, record transitions against it with a stable
//! `scope` / `from` / `to` attribute set plus any extras.

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Meter};

use subms_events::{Event, EventBridge};

/// Records categorical state transitions as a `u64` OTEL counter. Build it once
/// and reuse it; cloning the underlying instrument is cheap.
pub struct StateTransitionRecorder {
    counter: Counter<u64>,
}

impl StateTransitionRecorder {
    /// Build against an explicit meter.
    pub fn with_meter(
        meter: &Meter,
        counter_name: &'static str,
        description: &'static str,
    ) -> Self {
        let counter = meter
            .u64_counter(counter_name)
            .with_description(description)
            .build();
        Self { counter }
    }

    /// Build against the global meter provider. A no-op until a provider is
    /// installed, so it is always safe to construct.
    pub fn new(counter_name: &'static str, description: &'static str) -> Self {
        Self::with_meter(&global::meter("subms"), counter_name, description)
    }

    /// Record one transition. `scope` names what changed, `from`/`to` are the
    /// state tokens, `extra` carries any additional string attributes.
    pub fn record(&self, scope: &str, from: &str, to: &str, extra: &[(&str, String)]) {
        let mut attrs = Vec::with_capacity(3 + extra.len());
        attrs.push(KeyValue::new("scope", scope.to_string()));
        attrs.push(KeyValue::new("from", from.to_string()));
        attrs.push(KeyValue::new("to", to.to_string()));
        for (k, v) in extra {
            attrs.push(KeyValue::new(k.to_string(), v.clone()));
        }
        self.counter.add(1, &attrs);
    }
}

/// An `EventBridge` (from `subms-events`) that forwards every event to OTEL as a
/// `subms.events.total` counter, tagged with `topic` / `level` and, when present,
/// the `scope` / `from` / `to` transition attributes. Plug it into any
/// `subms-events` dispatcher with `dispatcher.add_bridge(Arc::new(OtelEventBridge::new()))`.
pub struct OtelEventBridge {
    counter: Counter<u64>,
}

impl OtelEventBridge {
    pub fn new() -> Self {
        let meter = global::meter("subms");
        let counter = meter
            .u64_counter("subms.events.total")
            .with_description("Count of subms-events events forwarded to OTEL")
            .build();
        Self { counter }
    }
}

impl Default for OtelEventBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBridge for OtelEventBridge {
    fn name(&self) -> &str {
        "otel"
    }
    fn forward(&self, event: &Event) {
        let mut attrs = vec![
            KeyValue::new("topic", event.topic.clone()),
            KeyValue::new("level", event.level.as_str()),
        ];
        for key in ["scope", "from", "to"] {
            if let Some(v) = event.attr(key) {
                attrs.push(KeyValue::new(key, v.to_string()));
            }
        }
        self.counter.add(1, &attrs);
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
