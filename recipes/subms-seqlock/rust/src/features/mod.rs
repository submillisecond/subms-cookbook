//! Opt-in feature catalog.

#[cfg(feature = "array")]
pub mod array;

#[cfg(feature = "latch")]
pub mod latch;

#[cfg(feature = "stats")]
pub mod stats;
