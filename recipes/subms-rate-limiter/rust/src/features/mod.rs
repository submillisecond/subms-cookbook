//! Opt-in feature catalog. Each submodule is gated by its own Cargo
//! feature flag and adds a focused capability to the base GCRA /
//! leaky-bucket limiter without bloating the core build.

// Shared utility: injected clock. Present whenever any feature is on
// so feature modules can share `Clock` / `TestClock` without each
// re-defining their own time source.
pub mod clock;

#[cfg(feature = "token-bucket")]
pub mod token_bucket;

// `hierarchical` composes a `TokenBucket` parent across child limiters,
// so it transitively pulls in `token-bucket` (Cargo enforces via the
// `hierarchical = ["token-bucket"]` declaration).
#[cfg(feature = "hierarchical")]
pub mod hierarchical;

#[cfg(feature = "distributed-backend")]
pub mod distributed_backend;

// `keyed` needs no clock injection - it drives the same monotonic origin the
// base limiter does, and its `_at` methods are the deterministic seam.
#[cfg(feature = "keyed")]
pub mod keyed;

// `metrics` wraps the `TokenBucket` shape (a counter model maps more
// directly to "current tokens" than the GCRA tat-tracking shape).
#[cfg(feature = "metrics")]
pub mod metrics;

pub use clock::{Clock, SystemClock, TestClock};
