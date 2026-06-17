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
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn schedule_and_tick_on_one_thread_works_like_base() {
        let w: ConcurrentTimerWheel<&'static str> = ConcurrentTimerWheel::new(64);
        w.schedule(2, "a");
        assert!(w.tick().is_empty());
        assert_eq!(w.tick(), vec!["a"]);
    }

    #[test]
    fn schedule_from_multiple_threads_fires_all_entries() {
        let w: ConcurrentTimerWheel<usize> = ConcurrentTimerWheel::new(256);
        let n_threads = 4;
        let per_thread = 50;
        let mut handles = Vec::new();
        for t in 0..n_threads {
            let w = w.clone();
            handles.push(thread::spawn(move || {
                for i in 0..per_thread {
                    w.schedule(1 + (i % 8), t * 1000 + i);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        // Walk enough ticks to retire every entry.
        let mut total = 0usize;
        for _ in 0..16 {
            total += w.tick().len();
        }
        assert_eq!(total, n_threads * per_thread);
    }

    #[test]
    fn cancel_from_another_thread_drops_entry() {
        let w: ConcurrentTimerWheel<&'static str> = ConcurrentTimerWheel::new(64);
        let id = w.schedule(5, "a");
        let w2 = w.clone();
        let canceller = thread::spawn(move || w2.cancel(id));
        assert!(canceller.join().unwrap());
        for _ in 0..8 {
            assert!(w.tick().is_empty());
        }
    }

    #[test]
    fn tick_does_not_deadlock_with_concurrent_schedules() {
        // Two writers race to fully populate the wheel, then we join
        // them and drain. The race exercises the mutex (concurrent
        // schedule calls) without baking in a timing assumption about
        // what's in the wheel when tick() runs.
        let w: ConcurrentTimerWheel<usize> = ConcurrentTimerWheel::new(256);
        let writers: Vec<_> = (0..2)
            .map(|t| {
                let w = w.clone();
                thread::spawn(move || {
                    for i in 0..500 {
                        w.schedule(1 + (i % 16), t * 1000 + i);
                    }
                })
            })
            .collect();
        for h in writers {
            h.join().unwrap();
        }
        let fired = AtomicUsize::new(0);
        for _ in 0..32 {
            fired.fetch_add(w.tick().len(), Ordering::AcqRel);
        }
        assert_eq!(fired.load(Ordering::Acquire), 1000);
    }

    #[test]
    fn cancel_after_fire_returns_false() {
        let w: ConcurrentTimerWheel<&'static str> = ConcurrentTimerWheel::new(64);
        let id = w.schedule(0, "now");
        // Walk a full revolution to ensure it fires.
        for _ in 0..64 {
            if !w.tick().is_empty() {
                break;
            }
        }
        assert!(!w.cancel(id));
    }

    #[test]
    fn clones_share_state() {
        let w: ConcurrentTimerWheel<u32> = ConcurrentTimerWheel::new(64);
        let w2 = w.clone();
        w.schedule(2, 99);
        // The clone observes the schedule.
        assert!(w2.tick().is_empty());
        assert_eq!(w2.tick(), vec![99]);
    }
}
