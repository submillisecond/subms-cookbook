//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability without bloating the
//! core build.
//!
//! See `Cargo.toml` `[features]` for the catalog.

#[cfg(feature = "bulk")]
pub mod bulk;

#[cfg(feature = "wait-strategies")]
pub mod wait_strategies;

#[cfg(feature = "mpsc-fan-in")]
pub mod mpsc_fan_in;

#[cfg(feature = "mpmc-disruptor")]
pub mod mpmc_disruptor;

#[cfg(feature = "metrics")]
pub mod metrics;
