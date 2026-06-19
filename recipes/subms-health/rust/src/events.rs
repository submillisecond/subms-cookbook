//! Status-change events. The registry emits a `subms-events` [`Event`] on
//! [`HEALTH_STATUS_TOPIC`] whenever the overall status, or any component's
//! status, transitions. Each event carries `scope` / `from` / `to` attributes
//! and a level derived from the new status.

use std::sync::Arc;

pub use subms_events::{
    DispatchMode, Event, EventBridge, EventBuilder, EventLevel, EventListener, listener,
};

use crate::status::HealthStatus;

/// Topic for health status-change events.
pub const HEALTH_STATUS_TOPIC: &str = "subms.health.status";

/// Map a status to the level of its transition event: a Down is an error, a
/// Degraded/Warn is a warning, an Up/Unknown is informational.
pub(crate) fn level_for(status: HealthStatus) -> EventLevel {
    match status {
        HealthStatus::Down => EventLevel::Error,
        HealthStatus::Degraded | HealthStatus::Warn => EventLevel::Warn,
        HealthStatus::Up | HealthStatus::Unknown => EventLevel::Info,
    }
}

/// Build a listener for health status-change events: `on_status_change(|e| ...)`.
/// Sugar over `subms_events::listener`.
pub fn on_status_change<F>(f: F) -> Arc<dyn EventListener>
where
    F: Fn(&Event) + Send + Sync + 'static,
{
    listener(f)
}
