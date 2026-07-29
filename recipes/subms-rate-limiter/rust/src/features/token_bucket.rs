//! Classic token bucket: capacity `C`, refill rate `R` tokens per
//! second. Tokens accumulate up to `C`; `try_acquire(n)` drains `n`
//! tokens and succeeds when `tokens >= n`.
//!
//! Different shape from the base GCRA / leaky-bucket: callers can drain
//! a variable batch in a single call, and the bucket can sit at full
//! capacity through periods of inactivity (the base GCRA pushes its
//! `tat` forward continuously). Useful when a request weight varies
//! per-call (e.g. "this query costs 5 tokens").
//!
//! State is held under a single `Mutex` for simplicity + correctness
//! across the `acquire(n)` + refill path. Single-atomic CAS-loop is
//! awkward when the result depends on `n`; the lock adds ~20 ns per
//! call uncontended on modern hardware.

use std::sync::Mutex;

use super::clock::{Clock, SystemClock};

struct State {
    /// Tokens currently in the bucket, scaled by `1_000_000_000` to
    /// avoid losing fractional refills between calls. A "real" token is
    /// `1_000_000_000` units here.
    tokens_scaled: u128,
    /// Last clock reading consumed in the refill calc.
    last_ns: u64,
}

pub struct TokenBucket {
    capacity: u64,
    /// Refill rate, tokens per second.
    rate_per_sec: f64,
    /// Scaled units of refill per ns. `rate_per_sec * 1_000_000_000 / 1_000_000_000`
    /// = `rate_per_sec` units per second; we store the per-ns delta to
    /// avoid floats in the hot path.
    units_per_ns_num: u128,
    units_per_ns_den: u128,
    clock: Box<dyn Clock>,
    state: Mutex<State>,
}

const SCALE: u128 = 1_000_000_000;

impl TokenBucket {
    /// Build a token bucket with `capacity` tokens, refilling at
    /// `rate_per_sec`. Starts full.
    pub fn new(capacity: u64, rate_per_sec: f64) -> Self {
        Self::with_clock(capacity, rate_per_sec, Box::new(SystemClock::new()))
    }

    /// Same as `new`, but the caller supplies the clock. Used by tests
    /// to drive a deterministic `TestClock`.
    pub fn with_clock(capacity: u64, rate_per_sec: f64, clock: Box<dyn Clock>) -> Self {
        let cap = capacity.max(1);
        let rate = rate_per_sec.max(0.0);
        // units_per_ns = (rate * SCALE) / 1_000_000_000. Stored as a
        // rational so the hot path stays in integer arithmetic.
        let num = (rate * SCALE as f64) as u128;
        let den = 1_000_000_000u128;
        let now = clock.now_ns();
        Self {
            capacity: cap,
            rate_per_sec: rate,
            units_per_ns_num: num,
            units_per_ns_den: den,
            clock,
            state: Mutex::new(State {
                tokens_scaled: (cap as u128).saturating_mul(SCALE),
                last_ns: now,
            }),
        }
    }

    /// Try to drain `n` tokens. Returns `true` if granted.
    pub fn try_acquire(&self, n: u64) -> bool {
        if n == 0 {
            return true;
        }
        let mut s = self.state.lock().unwrap();
        self.refill_locked(&mut s);
        let want = (n as u128).saturating_mul(SCALE);
        if s.tokens_scaled >= want {
            s.tokens_scaled -= want;
            true
        } else {
            false
        }
    }

    /// Shorthand for `try_acquire(1)`.
    pub fn try_acquire_one(&self) -> bool {
        self.try_acquire(1)
    }

    /// Current token count (whole tokens; fractional units truncated).
    pub fn available(&self) -> u64 {
        let mut s = self.state.lock().unwrap();
        self.refill_locked(&mut s);
        (s.tokens_scaled / SCALE) as u64
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn rate_per_sec(&self) -> f64 {
        self.rate_per_sec
    }

    fn refill_locked(&self, s: &mut State) {
        let now = self.clock.now_ns();
        let elapsed = now.saturating_sub(s.last_ns) as u128;
        if elapsed == 0 {
            return;
        }
        let add = elapsed.saturating_mul(self.units_per_ns_num) / self.units_per_ns_den;
        let cap_scaled = (self.capacity as u128).saturating_mul(SCALE);
        s.tokens_scaled = (s.tokens_scaled.saturating_add(add)).min(cap_scaled);
        s.last_ns = now;
    }
}

#[cfg(test)]
#[path = "token_bucket_tests.rs"]
mod tests;
