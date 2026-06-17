//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base k-way merge
//! iterator without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the recipe writeup
//! for per-feature p99 + use-case guidance.

#[cfg(feature = "seek-to")]
pub mod seek;

#[cfg(feature = "tombstones")]
pub mod tombstones;

#[cfg(feature = "dedup")]
pub mod dedup;

#[cfg(feature = "priority")]
pub mod priority;
