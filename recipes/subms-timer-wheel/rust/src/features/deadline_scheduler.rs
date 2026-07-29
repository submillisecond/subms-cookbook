//! Absolute-deadline scheduling layer on top of the base wheel.
//! Callers schedule against wall-clock instants ("fire at t=...") and
//! drive the scheduler with `poll()` calls. The wheel itself stays
//! tick-counted; the layer translates between instant deltas and tick
//! deltas via an injected `Clock` so the workload is deterministic
//! under test.
//!
//! The clock abstraction (a trait, not a free-running `Instant`) is
//! deliberate: time-based tests that sleep are flaky and slow; tests
//! that mutate a `TestClock` finish in microseconds and are exact.
//!
//! Granularity is 1 ms per tick by default; a deadline of `now + 12
//! ms` lands twelve ticks out. Sub-ms deadlines round up to one tick.

use crate::TimerWheel;
use std::time::{Duration, Instant};

/// Source of monotonic time. The deadline scheduler measures "from
/// now" deltas off this; production code injects [`MonotonicClock`]
/// and tests inject [`TestClock`].
pub trait Clock {
    /// Elapsed monotonic nanoseconds since the clock's origin. The
    /// origin doesn't matter; only deltas do.
    fn now_nanos(&self) -> u64;
}

#[derive(Default)]
pub struct MonotonicClock {
    origin: Option<Instant>,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            origin: Some(Instant::now()),
        }
    }
}

impl Clock for MonotonicClock {
    fn now_nanos(&self) -> u64 {
        let origin = self.origin.unwrap_or_else(Instant::now);
        Instant::now().duration_since(origin).as_nanos() as u64
    }
}

/// Hand-stepped clock for deterministic tests. `advance(d)` moves
/// time forward by `d`; the scheduler then catches up via `poll()`.
pub struct TestClock {
    now_nanos: std::cell::Cell<u64>,
}

impl Default for TestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TestClock {
    pub fn new() -> Self {
        Self {
            now_nanos: std::cell::Cell::new(0),
        }
    }

    pub fn advance(&self, d: Duration) {
        self.now_nanos
            .set(self.now_nanos.get().saturating_add(d.as_nanos() as u64));
    }
}

impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.now_nanos.get()
    }
}

pub struct DeadlineScheduler<V, C: Clock> {
    wheel: TimerWheel<V>,
    clock: C,
    tick_nanos: u64,
    /// Nanos consumed by previous ticks. Lets `poll()` advance
    /// `(elapsed - consumed) / tick_nanos` ticks atomically.
    consumed_nanos: u64,
}

impl<V, C: Clock> DeadlineScheduler<V, C> {
    /// Build a deadline scheduler with `num_slots` wheel slots and
    /// `tick` resolution (rounded up to 1 ns minimum).
    pub fn new(num_slots: usize, clock: C, tick: Duration) -> Self {
        let tick_nanos = (tick.as_nanos() as u64).max(1);
        Self {
            wheel: TimerWheel::new(num_slots),
            clock,
            tick_nanos,
            consumed_nanos: 0,
        }
    }

    pub fn tick_nanos(&self) -> u64 {
        self.tick_nanos
    }

    /// Schedule `value` to fire after `delay`. Equivalent to
    /// `schedule_at(now + delay, value)`.
    pub fn schedule_after(&mut self, delay: Duration, value: V) -> u64 {
        let ticks = self.nanos_to_ticks(delay.as_nanos() as u64);
        self.wheel.schedule(ticks, value)
    }

    /// Schedule `value` to fire at absolute deadline `when_nanos`
    /// (same epoch as `Clock::now_nanos`). If the deadline is in the
    /// past, the timer is queued for the next tick.
    pub fn schedule_at(&mut self, when_nanos: u64, value: V) -> u64 {
        let now = self.clock.now_nanos();
        let diff = when_nanos.saturating_sub(now);
        let ticks = self.nanos_to_ticks(diff).max(1);
        self.wheel.schedule(ticks, value)
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        self.wheel.cancel(id)
    }

    /// Advance the wheel by however many ticks the clock has accrued
    /// since the last `poll`. Returns every fired value across the
    /// catch-up batch. Idempotent if called twice with no clock
    /// movement in between.
    pub fn poll(&mut self) -> Vec<V> {
        let now = self.clock.now_nanos();
        let pending = now.saturating_sub(self.consumed_nanos);
        let ticks = (pending / self.tick_nanos) as usize;
        self.consumed_nanos = self
            .consumed_nanos
            .saturating_add(ticks as u64 * self.tick_nanos);
        let mut fired = Vec::new();
        for _ in 0..ticks {
            fired.extend(self.wheel.tick());
        }
        fired
    }

    fn nanos_to_ticks(&self, nanos: u64) -> usize {
        nanos.div_ceil(self.tick_nanos) as usize
    }
}

#[cfg(test)]
#[path = "deadline_scheduler_tests.rs"]
mod tests;
