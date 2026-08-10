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

use crate::{TimerError, TimerWheel};
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

    fn locked(&self) -> std::sync::MutexGuard<'_, TimerWheel<V>> {
        self.inner.lock().expect("timer-wheel mutex poisoned")
    }

    pub fn num_slots(&self) -> usize {
        self.locked().num_slots()
    }

    pub fn max_delay(&self) -> u64 {
        self.locked().max_delay()
    }

    pub fn pending(&self) -> usize {
        self.locked().pending()
    }

    pub fn is_empty(&self) -> bool {
        self.locked().is_empty()
    }

    pub fn slot_len(&self, slot: usize) -> usize {
        self.locked().slot_len(slot)
    }

    pub fn schedule(&self, delay_ticks: usize, value: V) -> u64 {
        self.locked().schedule(delay_ticks, value)
    }

    pub fn try_schedule(&self, delay_ticks: usize, value: V) -> Result<u64, TimerError> {
        self.locked().try_schedule(delay_ticks, value)
    }

    pub fn cancel(&self, id: u64) -> bool {
        self.locked().cancel(id)
    }

    pub fn reschedule(&self, id: u64, delay_ticks: usize) -> bool {
        self.locked().reschedule(id, delay_ticks)
    }

    /// Advance one tick. Returns the fired values; the mutex is
    /// released before the caller dispatches them.
    pub fn tick(&self) -> Vec<V> {
        self.locked().tick()
    }

    pub fn advance(&self, ticks: usize) -> Vec<V> {
        self.locked().advance(ticks)
    }

    pub fn drain(&self) -> Vec<V> {
        self.locked().drain()
    }

    pub fn clear(&self) {
        self.locked().clear()
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
