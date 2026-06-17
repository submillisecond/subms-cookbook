//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base cuckoo
//! filter without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the README's
//! "Features" section for per-feature p99 + memory cost + use cases.

#[cfg(feature = "variable-fingerprint")]
pub mod variable_fingerprint;

#[cfg(feature = "dynamic")]
pub mod dynamic;

#[cfg(feature = "concurrent-reads")]
pub mod concurrent_reads;

#[cfg(feature = "compressed-buckets")]
pub mod compressed_buckets;
