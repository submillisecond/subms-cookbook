//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base bloom
//! filter without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the README's
//! "Features" section for per-feature p99 + memory cost + use cases.

#[cfg(feature = "counting")]
pub mod counting;

// `scalable` builds on `counting` because each layer of a scalable
// filter needs the delete-supporting semantics. Cargo enforces the
// implies via `scalable = ["counting"]`.
#[cfg(feature = "scalable")]
pub mod scalable;

#[cfg(feature = "partitioned")]
pub mod partitioned;
