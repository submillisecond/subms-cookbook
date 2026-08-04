//! Exponentially-decaying histogram.
//!
//! Each bucket carries an effective count that decays toward zero
//! over time. Counts are multiplied by `0.5^(dt / halflife)` so the
//! distribution reflects recent activity more strongly than ancient
//! activity.
//!
//! Implementation: one `last_decay_ns` for the whole histogram. A
//! `record` first brings every bucket up to date when the clock has
//! moved, then increments; a percentile read applies the elapsed
//! factor as it sweeps. So `record` is O(1) only while time stands
//! still - once the clock advances it is O(buckets), which is why the
//! feature manifest classifies `decay` structural rather than
//! hot-path.
//!
//! Time source is an injected `Clock` trait so tests can drive the
//! clock deterministically. The production caller passes a wall
//! clock; tests pass `ManualClock`.

use crate::{index_of, value_from_index};

/// Time source. `now_ns` returns monotonic nanoseconds since some
/// arbitrary epoch.
pub trait Clock {
    fn now_ns(&self) -> u64;
}

/// Deterministic clock for tests. Move time forward with
/// `advance_ns`.
pub struct ManualClock {
    pub now: std::cell::Cell<u64>,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            now: std::cell::Cell::new(0),
        }
    }

    pub fn advance_ns(&self, dt: u64) {
        self.now.set(self.now.get() + dt);
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now_ns(&self) -> u64 {
        self.now.get()
    }
}

impl<C: Clock + ?Sized> Clock for &C {
    fn now_ns(&self) -> u64 {
        (*self).now_ns()
    }
}

/// Histogram with exponential decay. Counts are `f64` because decay
/// produces fractional values.
pub struct DecayingHdrHistogram<C: Clock> {
    sub_count_bits: u32,
    counters: Vec<f64>,
    high_index: usize,
    /// Wall-time at which the counters were last brought up to date.
    /// All counters reflect the world as of this timestamp.
    last_decay_ns: u64,
    /// Half-life in nanoseconds. A bucket's effective count halves
    /// every `halflife_ns` of elapsed time.
    halflife_ns: u64,
    clock: C,
}

impl<C: Clock> DecayingHdrHistogram<C> {
    /// New histogram with given precision, half-life (nanoseconds),
    /// and clock. A half-life of 1e9 means counts halve every second.
    pub fn new(significant_digits: u32, halflife_ns: u64, clock: C) -> Self {
        let sig = significant_digits.clamp(1, 5);
        let target = 2u32 * 10u32.pow(sig);
        let sub_count_bits = (32 - target.leading_zeros()).max(1);
        let sub_count = 1u32 << sub_count_bits;
        let last = clock.now_ns();
        Self {
            sub_count_bits,
            counters: vec![0.0; sub_count as usize],
            high_index: 0,
            last_decay_ns: last,
            halflife_ns: halflife_ns.max(1),
            clock,
        }
    }

    /// Record a value. Brings the whole counter array up to date
    /// before incrementing - so the new write competes fairly with
    /// the older, partly-decayed entries.
    pub fn record(&mut self, value: u64) {
        self.decay_to_now();
        let idx = index_of(value, self.sub_count_bits) as usize;
        if idx >= self.counters.len() {
            self.counters.resize(idx + 1, 0.0);
        }
        self.counters[idx] += 1.0;
        if idx > self.high_index {
            self.high_index = idx;
        }
    }

    /// Total effective count across all buckets.
    pub fn count(&self) -> f64 {
        let factor = self.peek_factor();
        self.counters.iter().sum::<f64>() * factor
    }

    pub fn max(&self) -> u64 {
        // Decay does not move the max bucket; once a value lands at
        // index i, the value-at-bucket-i remains the same. We just
        // need to find the highest bucket whose effective count is
        // still meaningfully > 0.
        for i in (0..=self.high_index.min(self.counters.len() - 1)).rev() {
            if self.counters[i] > 1e-9 {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        0
    }

    /// Value at the given quantile, computed against the decayed
    /// counts as of now.
    pub fn value_at_percentile(&self, q: f64) -> u64 {
        let total = self.count();
        if total <= 0.0 {
            return 0;
        }
        let target = (q.clamp(0.0, 1.0) * total).max(f64::MIN_POSITIVE);
        let factor = self.peek_factor();
        let mut cum = 0.0;
        let end = (self.high_index + 1).min(self.counters.len());
        for i in 0..end {
            cum += self.counters[i] * factor;
            if cum >= target {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        value_from_index(self.high_index, self.sub_count_bits)
    }

    pub fn halflife_ns(&self) -> u64 {
        self.halflife_ns
    }

    fn peek_factor(&self) -> f64 {
        let now = self.clock.now_ns();
        let dt = now.saturating_sub(self.last_decay_ns);
        if dt == 0 {
            return 1.0;
        }
        // factor = 0.5 ^ (dt / halflife)
        (-(dt as f64 / self.halflife_ns as f64) * std::f64::consts::LN_2).exp()
    }

    fn decay_to_now(&mut self) {
        let factor = self.peek_factor();
        if factor < 1.0 {
            for c in self.counters.iter_mut() {
                *c *= factor;
            }
            self.last_decay_ns = self.clock.now_ns();
        } else if factor == 1.0 && self.last_decay_ns == 0 {
            // First write after creation; pull last_decay forward.
            self.last_decay_ns = self.clock.now_ns();
        }
    }
}

#[cfg(test)]
#[path = "decay_tests.rs"]
mod tests;
