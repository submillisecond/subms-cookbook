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

use crate::{TimerError, TimerWheel};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimerMetrics {
    pub scheduled: u64,
    pub fired: u64,
    pub cancelled: u64,
    pub rescheduled: u64,
    pub drained: u64,
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

    pub fn max_delay(&self) -> u64 {
        self.wheel.max_delay()
    }

    pub fn pending(&self) -> usize {
        self.wheel.pending()
    }

    pub fn is_empty(&self) -> bool {
        self.wheel.is_empty()
    }

    pub fn slot_len(&self, slot: usize) -> usize {
        self.wheel.slot_len(slot)
    }

    pub fn metrics(&self) -> TimerMetrics {
        self.metrics
    }

    pub fn schedule(&mut self, delay_ticks: usize, value: V) -> u64 {
        self.metrics.scheduled += 1;
        self.wheel.schedule(delay_ticks, value)
    }

    pub fn try_schedule(&mut self, delay_ticks: usize, value: V) -> Result<u64, TimerError> {
        let id = self.wheel.try_schedule(delay_ticks, value)?;
        self.metrics.scheduled += 1;
        Ok(id)
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        let ok = self.wheel.cancel(id);
        if ok {
            self.metrics.cancelled += 1;
        }
        ok
    }

    pub fn reschedule(&mut self, id: u64, delay_ticks: usize) -> bool {
        let ok = self.wheel.reschedule(id, delay_ticks);
        if ok {
            self.metrics.rescheduled += 1;
        }
        ok
    }

    pub fn tick(&mut self) -> Vec<V> {
        self.metrics.ticks += 1;
        let fired = self.wheel.tick();
        self.metrics.fired += fired.len() as u64;
        fired
    }

    pub fn advance(&mut self, ticks: usize) -> Vec<V> {
        let mut fired = Vec::new();
        for _ in 0..ticks {
            fired.append(&mut self.tick());
        }
        fired
    }

    /// Hand back every pending timer. Counted separately from `fired`: a
    /// drained timer never came due, and folding the two together would make
    /// a shutdown look like a burst of expiries.
    pub fn drain(&mut self) -> Vec<V> {
        let out = self.wheel.drain();
        self.metrics.drained += out.len() as u64;
        out
    }

    pub fn clear(&mut self) {
        self.metrics.drained += self.wheel.pending() as u64;
        self.wheel.clear();
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
