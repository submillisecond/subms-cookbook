//! Opt-in HyperLogLog feature modules. Each entry is gated by its own
//! Cargo feature; the base `HyperLogLog` in `lib.rs` stays zero-dep.
//!
//! Composable: `sparse` SparseHyperLogLog promotes to a base
//! `HyperLogLog` once it crosses the dense-encoding threshold;
//! `union-intersect` works on any pair of base HLLs. The wire codec is
//! not here - it is default-path, in `src/codec.rs`.

#[cfg(feature = "sparse")]
pub mod sparse;
#[cfg(feature = "union-intersect")]
pub mod union_intersect;
