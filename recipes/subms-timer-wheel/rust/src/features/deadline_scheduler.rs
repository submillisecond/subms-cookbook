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
mod tests {
    use super::*;

    fn sched_with_clock() -> (DeadlineScheduler<&'static str, TestClock>, ()) {
        (
            DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1)),
            (),
        )
    }

    #[test]
    fn schedule_after_fires_after_elapsed_time() {
        let (mut s, _) = sched_with_clock();
        s.schedule_after(Duration::from_millis(3), "a");
        // 0 ms elapsed - nothing fires.
        assert!(s.poll().is_empty());
        // 2 ms elapsed - still nothing.
        s.clock.advance(Duration::from_millis(2));
        assert!(s.poll().is_empty());
        // 3 ms elapsed total - fires.
        s.clock.advance(Duration::from_millis(1));
        assert_eq!(s.poll(), vec!["a"]);
    }

    #[test]
    fn schedule_at_with_absolute_deadline_fires_when_clock_passes_it() {
        let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
        let when = s.clock.now_nanos() + Duration::from_millis(5).as_nanos() as u64;
        s.schedule_at(when, "five");
        s.clock.advance(Duration::from_millis(4));
        assert!(s.poll().is_empty());
        s.clock.advance(Duration::from_millis(1));
        assert_eq!(s.poll(), vec!["five"]);
    }

    #[test]
    fn schedule_at_in_the_past_fires_on_next_tick() {
        let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
        s.clock.advance(Duration::from_secs(10));
        let id = s.schedule_at(0, "stale");
        s.clock.advance(Duration::from_millis(1));
        assert_eq!(s.poll(), vec!["stale"]);
        // The id should be gone from cancellation tracking.
        assert!(!s.cancel(id));
    }

    #[test]
    fn cancel_removes_before_fire() {
        let (mut s, _) = sched_with_clock();
        let id = s.schedule_after(Duration::from_millis(3), "doomed");
        assert!(s.cancel(id));
        s.clock.advance(Duration::from_millis(10));
        assert!(s.poll().is_empty());
    }

    #[test]
    fn poll_with_no_clock_movement_is_idempotent() {
        let (mut s, _) = sched_with_clock();
        s.schedule_after(Duration::from_millis(2), "a");
        assert!(s.poll().is_empty());
        assert!(s.poll().is_empty());
        s.clock.advance(Duration::from_millis(2));
        let first = s.poll();
        let second = s.poll();
        assert_eq!(first, vec!["a"]);
        assert!(second.is_empty(), "second poll must not refire");
    }

    #[test]
    fn sub_tick_delay_rounds_up_to_one_tick() {
        let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
        // 500 us < 1 ms tick - must still fire on the next tick.
        s.schedule_after(Duration::from_micros(500), "a");
        s.clock.advance(Duration::from_millis(1));
        assert_eq!(s.poll(), vec!["a"]);
    }

    #[test]
    fn many_deadlines_fire_in_order() {
        let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
        for i in 1u32..=10 {
            s.schedule_after(Duration::from_millis(i as u64), i);
        }
        for i in 1u32..=10 {
            s.clock.advance(Duration::from_millis(1));
            assert_eq!(s.poll(), vec![i]);
        }
    }

    #[test]
    fn monotonic_clock_default_does_not_panic() {
        // Smoke test - production clock is hard to assert against; just
        // exercise the `now_nanos` path and confirm it monotonically
        // advances (or stays equal) across two calls.
        let c = MonotonicClock::new();
        let a = c.now_nanos();
        let b = c.now_nanos();
        assert!(b >= a);
    }
}
