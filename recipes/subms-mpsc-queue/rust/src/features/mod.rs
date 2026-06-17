//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base MPSC queue
//! without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the README's
//! "Features" section for per-feature p99 numbers and use cases.

#[cfg(feature = "mpmc")]
pub mod mpmc;

#[cfg(feature = "bounded")]
pub mod bounded;

#[cfg(feature = "batch")]
pub mod batch;

#[cfg(feature = "metrics")]
pub mod metrics;

#[cfg(feature = "affinity")]
pub mod affinity;
