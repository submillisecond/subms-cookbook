//! `subms-health` - a production health endpoint library, std-only and zero-dep.
//!
//! Register synchronous [`HealthIndicator`]s with a [`HealthRegistry`]. A stdlib
//! background thread probes each on its own cadence, off the request path, and
//! the registry serves a pre-rendered cached snapshot - so [`HealthRegistry::render`]
//! never touches a dependency and stays sub-ms.
//!
//! The headline piece is the [`EnvSection`] env/deploy provider: select env vars
//! by explicit key + prefix/glob, redact secrets, group them into a named
//! component. The JSON is deterministic and byte-equivalent to the Java + Python
//! ports.
//!
//! ```
//! use std::sync::Arc;
//! use subms_health::{HealthRegistry, RefreshPolicy, EnvSection, MapEnv, ComponentHealth, Clock, FixedClock};
//!
//! let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(1000, "2026-06-18T00:00:00Z"));
//! let mut reg = HealthRegistry::with_clock(clock);
//! reg.register_fn("db", RefreshPolicy::new(), || ComponentHealth::up().with_detail("ping", "ok"));
//!
//! let env = Arc::new(MapEnv::new().with("KICKSTART_ENV", "prod"));
//! reg.register(
//!     Arc::new(EnvSection::new("deploy").prefix("KICKSTART_").into_indicator(env)),
//!     RefreshPolicy::new(),
//! );
//!
//! let (code, json) = reg.render();
//! assert_eq!(code, 200);
//! assert!(json.contains("\"db\""));
//! ```

mod clock;
mod component;
mod env_section;
mod events;
mod json;
mod registry;
mod status;

#[cfg(feature = "harness")]
pub mod recipe;

#[cfg(feature = "otel")]
mod otel;
#[cfg(feature = "otel")]
pub use otel::OtelEventBridge;

pub use clock::{Clock, FixedClock, SystemClock};
pub use component::{ComponentHealth, FnIndicator, HealthIndicator, indicator};
pub use env_section::{
    EnvProvider, EnvSection, EnvSectionIndicator, MapEnv, RedactionPolicy, SystemEnv,
};
pub use events::{
    DispatchMode, Event, EventBridge, EventBuilder, EventLevel, EventListener, HEALTH_STATUS_TOPIC,
    listener, on_status_change,
};
pub use json::JsonValue;
pub use registry::{
    HealthConfig, HealthRegistry, ProbeKind, RefreshMode, RefreshPolicy, ServerIndicator,
};
pub use status::{HealthStatus, http_status_for};
