//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a specific capability to the base segment
//! reader without bloating the core build.
//!
//! See `Cargo.toml` `[features]` for the catalog and the cookbook page
//! for per-feature tradeoffs.

#[cfg(feature = "mmap")]
pub mod mmap;

#[cfg(feature = "crc32")]
pub mod crc32;

#[cfg(feature = "xxh3")]
pub mod xxh3;

#[cfg(feature = "lz4")]
pub mod lz4;

#[cfg(feature = "seek-index")]
pub mod seek_index;

#[cfg(feature = "wal-cursor")]
pub mod wal_cursor;
