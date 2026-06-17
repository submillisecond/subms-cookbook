//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base timer
//! wheel without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the README's
//! "Features" section for per-feature p99 + memory cost + use cases.

#[cfg(feature = "hierarchical")]
pub mod hierarchical;

#[cfg(feature = "concurrent")]
pub mod concurrent;

#[cfg(feature = "deadline-scheduler")]
pub mod deadline_scheduler;

#[cfg(feature = "cron")]
pub mod cron;

#[cfg(feature = "metrics")]
pub mod metrics;
