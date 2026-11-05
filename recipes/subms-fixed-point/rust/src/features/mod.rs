//! Opt-in feature catalog.

#[cfg(feature = "text")]
pub mod text;

#[cfg(feature = "money")]
pub mod money;

#[cfg(feature = "wide")]
pub mod wide;
