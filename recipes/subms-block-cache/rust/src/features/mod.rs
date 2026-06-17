//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base block cache
//! without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the README's
//! "Features" section for per-feature p99 + memory cost + use cases.

#[cfg(feature = "arc")]
pub mod arc;

#[cfg(feature = "tinylfu")]
pub mod tinylfu;

#[cfg(feature = "weighted")]
pub mod weighted;

#[cfg(feature = "concurrent-shards")]
pub mod concurrent_shards;

#[cfg(feature = "metrics")]
pub mod metrics;
