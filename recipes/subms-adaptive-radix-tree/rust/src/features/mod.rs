//! Opt-in ART feature modules. Each entry is gated by its own Cargo
//! feature; the base `Art` in `lib.rs` stays zero-dep.
//!
//! Composable: `serialize` writes a frozen tree; `range-scan` walks an
//! in-order slice; `concurrent-reads` hands readers a snapshot while a
//! writer holds the original; `metrics` wraps an `Art` with per-op
//! counters; `compaction` reclaims memory from over-allocated Full
//! nodes after deletions.

#[cfg(feature = "serialize")]
pub mod serialize;

#[cfg(feature = "range-scan")]
pub mod range_scan;

#[cfg(feature = "concurrent-reads")]
pub mod concurrent_reads;

#[cfg(feature = "metrics")]
pub mod metrics;

#[cfg(feature = "compaction")]
pub mod compaction;
