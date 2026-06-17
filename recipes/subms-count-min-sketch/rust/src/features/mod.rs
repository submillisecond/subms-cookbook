//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base sketch
//! without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog.

#[cfg(feature = "heavy-hitters")]
pub mod heavy_hitters;

#[cfg(feature = "windowed")]
pub mod windowed;

#[cfg(feature = "merge")]
pub mod merge;
