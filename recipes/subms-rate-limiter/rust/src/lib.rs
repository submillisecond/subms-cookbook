//! Lock-free rate limiter using the GCRA (Generic Cell Rate Algorithm) formulation.
//!
//! State is a single `AtomicU64` holding `tat_ns` - the theoretical arrival
//! time of the next slot. `try_acquire` reads `tat`, computes the new value
//! (`max(now, tat) + period`), and CAS-loops it in. Rejects when the new
//! `tat` would land more than `burst_ns` in the future.
//!
//! ```
//! use std::time::Duration;
//! use subms_rate_limiter::RateLimiter;
//!
//! // 1000 permits/sec, allow bursts of 10.
//! let rl = RateLimiter::new(1000.0, 10);
//! assert!(rl.try_acquire());
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Lock-free token-bucket / GCRA rate limiter.
pub struct RateLimiter {
    /// Theoretical arrival time of the next permit, in ns since the limiter
    /// was created.
    tat_ns: AtomicU64,
    /// ns per permit at the target rate. `1_000_000_000 / rate_per_sec`.
    period_ns: u64,
    /// Max burst ahead of now, in ns. `capacity * period_ns`.
    burst_ns: u64,
    /// Monotonic clock origin. All timestamps stored relative to this.
    origin: Instant,
}

impl RateLimiter {
    /// `rate_per_sec` permits per second, sustained. `burst_capacity` permits
    /// may be drawn in a burst before throttling kicks in.
    pub fn new(rate_per_sec: f64, burst_capacity: u64) -> Self {
        let period_ns = (1_000_000_000.0 / rate_per_sec) as u64;
        let burst_ns = period_ns.saturating_mul(burst_capacity);
        Self {
            tat_ns: AtomicU64::new(0),
            period_ns,
            burst_ns,
            origin: Instant::now(),
        }
    }

    /// Try to acquire one permit. Returns `true` if granted, `false` if the
    /// caller should be rejected (rate exceeded). Wait-free uncontended;
    /// CAS-loop under contention.
    pub fn try_acquire(&self) -> bool {
        let now = self.origin.elapsed().as_nanos() as u64;
        loop {
            let tat = self.tat_ns.load(Ordering::Acquire);
            // New TAT = max(now, tat) + period. Permit is allowed iff the new
            // TAT lands within `burst_ns` of `now`.
            let new_tat = tat.max(now).saturating_add(self.period_ns);
            if new_tat.saturating_sub(now) > self.burst_ns {
                return false;
            }
            // Race other producers for the slot.
            match self.tat_ns.compare_exchange_weak(
                tat,
                new_tat,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    /// Configured permits per second.
    pub fn rate_per_sec(&self) -> f64 {
        1_000_000_000.0 / self.period_ns as f64
    }

    /// Configured burst capacity (in permits).
    pub fn burst_capacity(&self) -> u64 {
        self.burst_ns.checked_div(self.period_ns).unwrap_or(0)
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each module is gated on its own Cargo
// feature; the base GCRA limiter stays zero-dep + std-only.
#[cfg(any(
    feature = "token-bucket",
    feature = "hierarchical",
    feature = "distributed-backend",
    feature = "metrics",
))]
pub mod features;

#[cfg(any(
    feature = "token-bucket",
    feature = "hierarchical",
    feature = "distributed-backend",
    feature = "metrics",
))]
pub use features::clock::{Clock, SystemClock, TestClock};

#[cfg(feature = "distributed-backend")]
pub use features::distributed_backend::{Backend, DistributedLimiter, InMemoryBackend};
#[cfg(feature = "hierarchical")]
pub use features::hierarchical::HierarchicalLimiter;
#[cfg(feature = "metrics")]
pub use features::metrics::{MeteredTokenBucket, MetricsSnapshot};
#[cfg(feature = "token-bucket")]
pub use features::token_bucket::TokenBucket;
