//! Optional OpenTelemetry bridge (`otel` feature). Routes through the cookbook's
//! `subms-otel` recipe: its `OtelEventBridge` is a `subms-events` bridge that
//! forwards every status-change event to OTEL as a counter. There is no direct
//! opentelemetry dependency here.
//!
//! ```ignore
//! use std::sync::Arc;
//! use subms_health::{HealthRegistry, OtelEventBridge};
//! let mut reg = HealthRegistry::new();
//! reg.add_bridge(Arc::new(OtelEventBridge::new()));
//! ```

pub use subms_otel::OtelEventBridge;
