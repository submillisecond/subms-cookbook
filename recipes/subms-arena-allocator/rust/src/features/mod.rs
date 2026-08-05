//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base bump arena
//! without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the cookbook page
//! for the per-feature p99 + memory cost + use cases.

#[cfg(feature = "typed")]
pub mod typed;

#[cfg(feature = "growable")]
pub mod growable;

#[cfg(feature = "stats")]
pub mod stats;

#[cfg(feature = "aligned")]
pub mod aligned;
