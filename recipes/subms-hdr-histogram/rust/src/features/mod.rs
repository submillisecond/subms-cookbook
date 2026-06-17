//! Opt-in feature modules. Each entry is gated by its own Cargo
//! feature; the base `HdrHistogram` in `lib.rs` stays zero-dep + std-only.

#[cfg(feature = "concurrent-writes")]
pub mod concurrent_writes;

// `dual-recorder` builds on `concurrent-writes` because the active
// histogram must accept concurrent producer writes while the consumer
// hot-swaps. Cargo enforces the implies via `dual-recorder =
// ["concurrent-writes"]`.
#[cfg(feature = "dual-recorder")]
pub mod dual_recorder;

#[cfg(feature = "merge")]
pub mod merge;

#[cfg(feature = "decay")]
pub mod decay;

#[cfg(feature = "value-tagging")]
pub mod value_tagging;

#[cfg(feature = "iterators")]
pub mod iterators;
