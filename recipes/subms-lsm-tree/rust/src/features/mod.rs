//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base LSM tree
//! without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the README's
//! "Features" section for per-feature p99 + memory cost + use cases.

#[cfg(feature = "wal")]
pub mod wal;

#[cfg(feature = "tiered-compaction")]
pub mod tiered_compaction;

#[cfg(feature = "leveled-compaction")]
pub mod leveled_compaction;

#[cfg(feature = "snapshot")]
pub mod snapshot;

#[cfg(feature = "lz4")]
pub mod lz4;

#[cfg(feature = "zstd")]
pub mod zstd;

#[cfg(feature = "block-cache-integration")]
pub mod block_cache_integration;
