//! Lock-free rate limiter using the GCRA (Generic Cell Rate Algorithm) formulation.
//!
//! State is a single `AtomicU64` holding `tat_ns` - the theoretical arrival
//! time of the next slot. `try_acquire` reads `tat`, computes the new value
//! (`max(now, tat) + period`), and CAS-loops it in. Rejects when the new
//! `tat` would land more than `burst_ns` in the future.
//!
//! ```
//! use subms_rate_limiter::RateLimiter;
//!
//! // 1000 permits/sec, allow bursts of 10.
//! let rl = RateLimiter::new(1000.0, 10);
//! assert!(rl.try_acquire());
//! ```
//!
//! Thread-safety: [`RateLimiter`] is `Send + Sync` and every method takes
//! `&self`. Share one instance across threads behind an `Arc`; there is no
//! interior lock and no `&mut self` path.
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-rate-limiter>

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Outcome of [`RateLimiter::try_acquire_with_retry`]: a permit was granted, or
/// the caller should wait at least `Retry(d)` before a retry will conform - the
/// value for an HTTP `Retry-After`. Under contention the duration is a
/// best-effort hint (another thread may take the slot first), the guarantee
/// every lock-free rate limiter's retry-after carries.
///
/// `Unattainable` is the typed answer to a request no amount of waiting can
/// satisfy: `n` above `burst_capacity` overshoots the burst window even from a
/// fully idle limiter, so it is a sizing error rather than backpressure.
/// `governor` reports the same condition as `InsufficientCapacity`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Acquire {
    Ok,
    Retry(Duration),
    Unattainable { burst_capacity: u64 },
}

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
    /// may be drawn in a burst before throttling kicks in. A capacity of 0 is
    /// floored to 1 - a window of zero rejects every request including the
    /// first, which is a broken limiter rather than a strict one.
    pub fn new(rate_per_sec: f64, burst_capacity: u64) -> Self {
        let period_ns = (1_000_000_000.0 / rate_per_sec) as u64;
        let burst_ns = period_ns.saturating_mul(burst_capacity.max(1));
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
        self.try_acquire_at(self.now_ns())
    }

    /// `try_acquire` against a caller-supplied `now` (ns since [`Self::now_ns`]'s
    /// origin) instead of the internal monotonic clock. This is the driven-time
    /// entry point: a simulation, a replay harness or a deterministic test steps
    /// `now` itself rather than sleeping on the wall clock. `governor` exposes
    /// the same idea as `check_at`.
    pub fn try_acquire_at(&self, now: u64) -> bool {
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

    /// Like [`Self::try_acquire`], but on rejection reports how long to wait
    /// before a retry will conform - the value for an HTTP `Retry-After`. A
    /// grant advances the limiter exactly as `try_acquire` does; a rejection
    /// leaves it untouched.
    pub fn try_acquire_with_retry(&self) -> Acquire {
        self.try_acquire_with_retry_at(self.now_ns())
    }

    /// `try_acquire_with_retry` against a caller-supplied `now`.
    pub fn try_acquire_with_retry_at(&self, now: u64) -> Acquire {
        self.try_acquire_n_with_retry_at(now, 1)
    }

    /// Draw `n` permits at once - a weighted request, where a heavy message
    /// costs more of the budget than a light one. All-or-nothing: a rejected
    /// call spends nothing. `n` above [`Self::burst_capacity`] can never be
    /// granted; use [`Self::try_acquire_n_with_retry`] to see that as a typed
    /// outcome instead of a bare `false`.
    pub fn try_acquire_n(&self, n: u64) -> bool {
        self.try_acquire_n_at(self.now_ns(), n)
    }

    /// [`Self::try_acquire_n`] against a caller-supplied `now`.
    pub fn try_acquire_n_at(&self, now: u64, n: u64) -> bool {
        matches!(self.try_acquire_n_with_retry_at(now, n), Acquire::Ok)
    }

    /// [`Self::try_acquire_n`] reporting the retry-after on rejection.
    pub fn try_acquire_n_with_retry(&self, n: u64) -> Acquire {
        self.try_acquire_n_with_retry_at(self.now_ns(), n)
    }

    /// The weighted GCRA step: one request of weight `n` costs `n` periods of
    /// theoretical arrival time. `n = 0` is a free probe that neither advances
    /// the limiter nor can be rejected.
    pub fn try_acquire_n_with_retry_at(&self, now: u64, n: u64) -> Acquire {
        if n == 0 {
            return Acquire::Ok;
        }
        let cost = self.period_ns.saturating_mul(n);
        if cost > self.burst_ns {
            return Acquire::Unattainable {
                burst_capacity: self.burst_capacity(),
            };
        }
        loop {
            let tat = self.tat_ns.load(Ordering::Acquire);
            let new_tat = tat.max(now).saturating_add(cost);
            if new_tat.saturating_sub(now) > self.burst_ns {
                // Rejected: wait until the slot re-enters the burst window.
                let wait = new_tat.saturating_sub(self.burst_ns).saturating_sub(now);
                return Acquire::Retry(Duration::from_nanos(wait));
            }
            match self.tat_ns.compare_exchange_weak(
                tat,
                new_tat,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Acquire::Ok,
                Err(_) => continue,
            }
        }
    }

    /// How long until `n` permits would conform, without taking them.
    /// `Some(ZERO)` means a call right now would be granted; `None` means `n`
    /// exceeds the burst capacity and no wait will help. Read-only: unlike
    /// `try_acquire`, this never advances the limiter, so a scheduler can plan
    /// against it without spending budget.
    pub fn time_until_ready(&self, n: u64) -> Option<Duration> {
        self.time_until_ready_at(self.now_ns(), n)
    }

    /// [`Self::time_until_ready`] against a caller-supplied `now`.
    pub fn time_until_ready_at(&self, now: u64, n: u64) -> Option<Duration> {
        if n == 0 {
            return Some(Duration::ZERO);
        }
        let cost = self.period_ns.saturating_mul(n);
        if cost > self.burst_ns {
            return None;
        }
        let tat = self.tat_ns.load(Ordering::Acquire);
        let new_tat = tat.max(now).saturating_add(cost);
        if new_tat.saturating_sub(now) > self.burst_ns {
            let wait = new_tat.saturating_sub(self.burst_ns).saturating_sub(now);
            Some(Duration::from_nanos(wait))
        } else {
            Some(Duration::ZERO)
        }
    }

    /// Block until `n` permits are granted or `timeout` elapses, whichever
    /// comes first. Returns `false` without sleeping when the wait provably
    /// exceeds the timeout, matching Guava's `tryAcquire(permits, timeout)`.
    ///
    /// Waiters are not queued, so this is not FIFO: several blocked callers
    /// wake and race for the same slot. It sleeps by design and is outside the
    /// per-op sub-ms claim.
    pub fn acquire_within(&self, n: u64, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            match self.try_acquire_n_with_retry(n) {
                Acquire::Ok => return true,
                Acquire::Unattainable { .. } => return false,
                Acquire::Retry(wait) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if wait > remaining {
                        return false;
                    }
                    std::thread::sleep(wait);
                }
            }
        }
    }

    /// Drop all accumulated throttle state: the next `burst_capacity` permits
    /// are granted immediately. For a session that reconnects and gets a fresh
    /// allowance from the venue, or a test that reuses one limiter.
    pub fn reset(&self) {
        self.tat_ns.store(0, Ordering::Release);
    }

    /// Nanoseconds elapsed on the limiter's own monotonic clock. The value the
    /// `_at` methods expect, so a caller can read the clock once and reuse it
    /// across several limiters.
    pub fn now_ns(&self) -> u64 {
        self.origin.elapsed().as_nanos() as u64
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
    feature = "keyed",
))]
pub mod features;

#[cfg(any(
    feature = "token-bucket",
    feature = "hierarchical",
    feature = "distributed-backend",
    feature = "metrics",
    feature = "keyed",
))]
pub use features::clock::{Clock, SystemClock, TestClock};

#[cfg(feature = "distributed-backend")]
pub use features::distributed_backend::{Backend, DistributedLimiter, InMemoryBackend};
#[cfg(feature = "hierarchical")]
pub use features::hierarchical::HierarchicalLimiter;
#[cfg(feature = "keyed")]
pub use features::keyed::KeyedRateLimiter;
#[cfg(feature = "metrics")]
pub use features::metrics::{MeteredTokenBucket, MetricsSnapshot};
#[cfg(feature = "token-bucket")]
pub use features::token_bucket::TokenBucket;

#[cfg(test)]
#[path = "rate_limiter_tests.rs"]
mod rate_limiter_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
