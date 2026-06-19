//! `ComponentHealth` (one health node) + the synchronous `HealthIndicator`
//! contract probed off the request path by the registry's background refresher.

use std::collections::BTreeMap;

use crate::json::{JsonValue, push_component};
use crate::status::HealthStatus;

/// One node in the health tree: a status, a flat `details` map, and optional
/// nested `components`. Maps are sorted (BTreeMap) so the JSON is deterministic.
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub details: BTreeMap<String, JsonValue>,
    pub components: BTreeMap<String, ComponentHealth>,
}

impl ComponentHealth {
    pub fn new(status: HealthStatus) -> Self {
        Self {
            status,
            details: BTreeMap::new(),
            components: BTreeMap::new(),
        }
    }

    pub fn up() -> Self {
        Self::new(HealthStatus::Up)
    }

    pub fn unknown() -> Self {
        Self::new(HealthStatus::Unknown)
    }

    pub fn down(reason: impl Into<String>) -> Self {
        let mut c = Self::new(HealthStatus::Down);
        c.details
            .insert("error".to_string(), JsonValue::Str(reason.into()));
        c
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        let mut c = Self::new(HealthStatus::Degraded);
        c.details
            .insert("error".to_string(), JsonValue::Str(reason.into()));
        c
    }

    pub fn with_detail(mut self, key: &str, value: impl Into<JsonValue>) -> Self {
        self.details.insert(key.to_string(), value.into());
        self
    }

    pub fn with_subcomponent(mut self, name: &str, child: ComponentHealth) -> Self {
        self.components.insert(name.to_string(), child);
        self
    }

    /// Worst of this node's own status and every nested subcomponent.
    pub fn effective_status(&self) -> HealthStatus {
        let mut acc = self.status;
        for c in self.components.values() {
            acc = acc.worse(c.effective_status());
        }
        acc
    }

    /// Serialise this component standalone (no registry timing fields).
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        push_component(&mut out, self);
        out
    }
}

/// A health probe. Synchronous by design: a probe may block (a DB ping), so the
/// registry runs it on its background refresher, never on the request path.
pub trait HealthIndicator: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self) -> ComponentHealth;
}

/// Closure-backed indicator so trivial probes need no struct.
pub struct FnIndicator<F> {
    name: String,
    f: F,
}

impl<F: Fn() -> ComponentHealth + Send + Sync> HealthIndicator for FnIndicator<F> {
    fn name(&self) -> &str {
        &self.name
    }
    fn check(&self) -> ComponentHealth {
        (self.f)()
    }
}

/// Build a closure indicator: `indicator("db", || ComponentHealth::up())`.
pub fn indicator<F: Fn() -> ComponentHealth + Send + Sync>(name: &str, f: F) -> FnIndicator<F> {
    FnIndicator {
        name: name.to_string(),
        f,
    }
}
