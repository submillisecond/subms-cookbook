//! Single-level hashed timer wheel. O(1) schedule and cancel.
//!
//! The wheel has `N` buckets. A scheduled timer at `delay` ticks goes into
//! bucket `(now + delay) % N` with a `rounds` counter of `delay / N`.
//! On `tick()`, the hand walks one bucket forward: timers with `rounds == 0`
//! fire (their callbacks are returned); the rest have `rounds` decremented.
//! Cancel sets a flag; the entry is dropped lazily on the next visit.
//!
//! Tradeoff vs the hierarchical wheel: with a single level, a long delay
//! causes many no-op revolutions. For workloads with delays bounded by
//! `N` ticks, the single level is optimal.
//!
//! ```
//! use subms_timer_wheel::TimerWheel;
//! let mut w: TimerWheel<&'static str> = TimerWheel::new(256);
//! let id = w.schedule(5, "hello");
//! for _ in 0..4 { assert!(w.tick().is_empty()); }
//! assert_eq!(w.tick(), vec!["hello"]);
//! let _ = id; // returned id can be used to cancel before firing
//! ```

use std::collections::HashMap;

pub struct TimerWheel<V> {
    slots: Vec<Slot<V>>,
    mask: usize,
    hand: usize,
    next_id: u64,
    /// Auxiliary index for O(1) cancellation: id -> (slot, position-in-vec).
    /// Position can shift when entries fire; we re-index on tick to keep it tight.
    id_to_slot: HashMap<u64, usize>,
}

struct Slot<V> {
    entries: Vec<Entry<V>>,
}

struct Entry<V> {
    id: u64,
    rounds: u32,
    value: V,
    cancelled: bool,
}

impl<V> TimerWheel<V> {
    /// `num_slots` rounded up to a power of two.
    pub fn new(num_slots: usize) -> Self {
        let n = num_slots.max(2).next_power_of_two();
        let mut slots = Vec::with_capacity(n);
        for _ in 0..n {
            slots.push(Slot {
                entries: Vec::new(),
            });
        }
        Self {
            slots,
            mask: n - 1,
            hand: 0,
            next_id: 1,
            id_to_slot: HashMap::new(),
        }
    }

    pub fn num_slots(&self) -> usize {
        self.slots.len()
    }

    /// Number of live (scheduled, not yet fired or cancelled) timers - the size
    /// of the id->slot index. A correct wheel returns this to ~0 once every
    /// scheduled timer has fired; a leak would let it climb without bound.
    pub fn pending(&self) -> usize {
        self.id_to_slot.len()
    }

    /// Schedule `value` to fire in `delay_ticks`. Returns an id for cancel.
    pub fn schedule(&mut self, delay_ticks: usize, value: V) -> u64 {
        let target = self.hand.wrapping_add(delay_ticks);
        let slot = target & self.mask;
        let rounds = (delay_ticks / self.slots.len()) as u32;
        let id = self.next_id;
        self.next_id += 1;
        self.slots[slot].entries.push(Entry {
            id,
            rounds,
            value,
            cancelled: false,
        });
        self.id_to_slot.insert(id, slot);
        id
    }

    /// Mark a scheduled timer cancelled. Returns `true` if it was pending.
    pub fn cancel(&mut self, id: u64) -> bool {
        let Some(&slot) = self.id_to_slot.get(&id) else {
            return false;
        };
        // Walk the slot's vec linearly; lazy-flag the entry. Cancellation in
        // O(1) amortised; the actual sweep is on the next tick of this slot.
        for e in &mut self.slots[slot].entries {
            if e.id == id && !e.cancelled {
                e.cancelled = true;
                return true;
            }
        }
        false
    }

    /// Advance the hand one tick. Returns the values of all timers that
    /// fired (rounds was 0 and not cancelled). Cancelled timers are dropped
    /// silently. Other timers have their `rounds` decremented.
    pub fn tick(&mut self) -> Vec<V> {
        self.hand = (self.hand + 1) & self.mask;
        let slot = self.hand;
        let mut fired = Vec::new();
        let entries = std::mem::take(&mut self.slots[slot].entries);
        let mut survivors = Vec::new();
        for mut e in entries {
            if e.cancelled {
                self.id_to_slot.remove(&e.id);
                continue;
            }
            if e.rounds == 0 {
                self.id_to_slot.remove(&e.id);
                fired.push(e.value);
            } else {
                e.rounds -= 1;
                survivors.push(e);
            }
        }
        self.slots[slot].entries = survivors;
        fired
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Each is independent of the base wheel and
// gated by its own Cargo feature; `cargo add subms-timer-wheel` alone
// keeps the base zero-dep + std-only shape.
#[cfg(any(
    feature = "hierarchical",
    feature = "concurrent",
    feature = "deadline-scheduler",
    feature = "cron",
    feature = "metrics",
))]
pub mod features;

#[cfg(feature = "concurrent")]
pub use features::concurrent::ConcurrentTimerWheel;
#[cfg(feature = "cron")]
pub use features::cron::{CronError, CronSchedule, CronScheduler};
#[cfg(feature = "deadline-scheduler")]
pub use features::deadline_scheduler::{Clock, DeadlineScheduler, MonotonicClock, TestClock};
#[cfg(feature = "hierarchical")]
pub use features::hierarchical::HierarchicalTimerWheel;
#[cfg(feature = "metrics")]
pub use features::metrics::{MeteredTimerWheel, TimerMetrics};

#[cfg(test)]
#[path = "wheel_tests.rs"]
mod wheel_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
