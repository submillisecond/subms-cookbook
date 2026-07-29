//! # subms-stats
//!
//! Latency-distribution statistics for measurement-heavy workloads.
//! Pure functions on `&[u64]` sample arrays + an ergonomic
//! [`SubMsSamples`] facade.
//!
//! This is the statistics primitive the `subms` perf harness uses
//! internally, published as its own crate so you can use it without
//! pulling in the bench machinery. Bring your own `Vec<u64>` of
//! nanosecond readings (from any source) and compose the analyses you
//! need.
//!
//! ```
//! use subms_stats::SubMsSamples;
//!
//! let raw = vec![100u64, 200, 150, 300, 250, 175, 125, 400];
//! let s = SubMsSamples::new(&raw);
//! assert!(s.p99() >= s.p50());   // the tail never sits below the median
//! assert!(s.max() >= s.p99());   // max bounds every percentile
//! ```
//!
//! ## Feature flags
//!
//! The crate ships a tiny **core** that's always on (`percentile`,
//! `mean`, `stddev`) plus optional features that you opt into via Cargo:
//!
//! - `histogram` (default) - log2-spaced CDF buckets
//! - `jitter`    (default) - per-window variance score
//! - `tail`      (default) - CTE, Hill estimator, fatness ratio
//! - `robust`    (default) - IQR, MAD, CoV, skewness, kurtosis
//! - `compare`   (default) - KS statistic, Cohen's d
//! - `bootstrap` (default) - bootstrap CIs for percentiles
//!
//! Everything is on by default. For a minimal build:
//!
//! ```toml
//! [dependencies]
//! subms-stats = { version = "0.7.0", default-features = false }
//! ```
//!
//! Then pick only the features you need:
//!
//! ```toml
//! subms-stats = { version = "0.7.0", default-features = false, features = ["histogram", "tail"] }
//! ```

// Always-on core.
pub mod percentiles;
pub mod samples;

#[cfg(feature = "bootstrap")]
pub mod bootstrap;
#[cfg(feature = "compare")]
pub mod compare;
#[cfg(feature = "histogram")]
pub mod histogram;
#[cfg(feature = "jitter")]
pub mod jitter;
#[cfg(feature = "robust")]
pub mod robust;
#[cfg(feature = "tail")]
pub mod tail;

// Flat re-exports for the most common entry points. Library consumers
// can use module paths (e.g. `subms_stats::tail::hill_tail_index`) or
// the flat form (e.g. `subms_stats::hill_tail_index`).
pub use percentiles::{mean, percentile, percentile_sweep, stddev};
pub use samples::SubMsSamples;

#[cfg(feature = "bootstrap")]
pub use bootstrap::bootstrap_percentile_ci;
#[cfg(feature = "compare")]
pub use compare::{cohens_d, ks_statistic};
#[cfg(feature = "histogram")]
pub use histogram::cdf_buckets;
#[cfg(feature = "jitter")]
pub use jitter::jitter_score;
#[cfg(feature = "robust")]
pub use robust::{coefficient_of_variation, iqr, kurtosis, median_absolute_deviation, skewness};
#[cfg(feature = "tail")]
pub use tail::{conditional_tail_expectation, hill_tail_index, tail_fatness_ratio};
