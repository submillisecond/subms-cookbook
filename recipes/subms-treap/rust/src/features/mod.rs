//! Opt-in treap feature catalog. Each submodule is gated by its own
//! Cargo feature flag and adds a focused capability without bloating
//! the base treap build.
//!
//! See `Cargo.toml` `[features]` for the catalog + the cookbook page
//! for the per-feature p99 + memory cost + composition guidance.

#[cfg(feature = "persistent")]
pub mod persistent;

#[cfg(feature = "merge-split")]
pub mod merge_split;

#[cfg(feature = "concurrent-reads")]
pub mod concurrent_reads;
