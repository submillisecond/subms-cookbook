//! Thread-safe timer wheel: short-mutex wrapper around the base
//! `TimerWheel`. Schedule + cancel + tick all serialize on a single
//! `Mutex` because the critical sections are O(1) (or O(slot) on
//! tick - bounded by entries-in-bucket, typically tiny).
//!
//! Why a mutex and not a lock-free or sharded design: timer-wheel
//! operations are short enough that a contended mutex still wins on
//! tail latency vs the cache-line ping-pong of an atomic-list shape,
//! provided callers don't hold long external locks while inside a
//! callback. The `tick()` method returns the fired values out of the
//! critical section, so a caller can release the lock between
//! retrieval and dispatch.
//!
//! Tradeoff vs the base wheel: every operation pays a lock + unlock.
//! For single-threaded workloads, prefer the base `TimerWheel`.

use crate::TimerWheel;
use std::sync::{Arc, Mutex};

pub struct ConcurrentTimerWheel<V> {
    inner: Arc<Mutex<TimerWheel<V>>>,
}

impl<V> ConcurrentTimerWheel<V> {
    pub fn new(num_slots: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TimerWheel::new(num_slots))),
        }
    }

    pub fn num_slots(&self) -> usize {
        self.inner
            .lock()
            .expect("timer-wheel mutex poisoned")
            .num_slots()
    }

    pub fn schedule(&self, delay_ticks: usize, value: V) -> u64 {
        self.inner
            .lock()
            .expect("timer-wheel mutex poisoned")
            .schedule(delay_ticks, value)
    }

    pub fn cancel(&self, id: u64) -> bool {
        self.inner
            .lock()
            .expect("timer-wheel mutex poisoned")
            .cancel(id)
    }

    /// Advance one tick. Returns the fired values; the mutex is
    /// released before the caller dispatches them.
    pub fn tick(&self) -> Vec<V> {
        self.inner
            .lock()
            .expect("timer-wheel mutex poisoned")
            .tick()
    }
}

impl<V> Clone for ConcurrentTimerWheel<V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
#[path = "concurrent_tests.rs"]
mod tests;
