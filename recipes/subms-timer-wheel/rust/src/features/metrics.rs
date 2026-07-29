//! Metered timer wheel: thin wrapper around the base `TimerWheel`
//! that tracks per-instance counters. Counters are plain `u64`
//! fields - no atomics, no locks - because the underlying wheel is
//! itself single-threaded. Pair with the `concurrent` feature for a
//! thread-safe metered surface (wrap a `MeteredTimerWheel` inside a
//! mutex of your own).
//!
//! `cascade_events` is always 0 for the single-level base wheel.
//! It's tracked here so downstream code that swaps the wheel for
//! the hierarchical variant doesn't need a schema change in the
//! metrics snapshot. The hierarchical wheel exposes its own
//! `cascades()` counter directly; see that module.

use crate::TimerWheel;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimerMetrics {
    pub scheduled: u64,
    pub fired: u64,
    pub cancelled: u64,
    pub ticks: u64,
    pub cascade_events: u64,
}

pub struct MeteredTimerWheel<V> {
    wheel: TimerWheel<V>,
    metrics: TimerMetrics,
}

impl<V> MeteredTimerWheel<V> {
    pub fn new(num_slots: usize) -> Self {
        Self {
            wheel: TimerWheel::new(num_slots),
            metrics: TimerMetrics::default(),
        }
    }

    pub fn num_slots(&self) -> usize {
        self.wheel.num_slots()
    }

    pub fn metrics(&self) -> TimerMetrics {
        self.metrics
    }

    pub fn schedule(&mut self, delay_ticks: usize, value: V) -> u64 {
        self.metrics.scheduled += 1;
        self.wheel.schedule(delay_ticks, value)
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        let ok = self.wheel.cancel(id);
        if ok {
            self.metrics.cancelled += 1;
        }
        ok
    }

    pub fn tick(&mut self) -> Vec<V> {
        self.metrics.ticks += 1;
        let fired = self.wheel.tick();
        self.metrics.fired += fired.len() as u64;
        fired
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
