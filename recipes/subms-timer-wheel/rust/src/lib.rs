//! Single-level hashed timer wheel. O(1) schedule and cancel.
//!
//! The wheel has `N` buckets. A scheduled timer at `delay` ticks goes into
//! bucket `(now + delay) % N` with a `rounds` counter of how many full
//! revolutions it must sit out first. On `tick()`, the hand walks one bucket
//! forward: timers with `rounds == 0` fire (their values are returned); the
//! rest have `rounds` decremented. Cancel drops the id from the index and
//! flags the entry; the flagged entry is reclaimed on the next visit to its
//! bucket.
//!
//! Tradeoff vs the hierarchical wheel: with a single level, a long delay
//! causes many no-op revolutions. For workloads with delays bounded by
//! `N` ticks, the single level is optimal.
//!
//! Thread safety: `TimerWheel` is single-threaded. Every method takes
//! `&mut self`, so one caller owns the wheel and there is no interior
//! synchronisation to pay for. It is `Send` when `V: Send` and can be moved
//! to a ticker thread; to arm timers from several threads at once, enable the
//! `concurrent` feature or hand work to the ticker thread through a queue.
//!
//! ```
//! use subms_timer_wheel::TimerWheel;
//! let mut w: TimerWheel<&'static str> = TimerWheel::new(256);
//! let id = w.schedule(5, "hello");
//! for _ in 0..4 { assert!(w.tick().is_empty()); }
//! assert_eq!(w.tick(), vec!["hello"]);
//! let _ = id; // returned id can be used to cancel before firing
//! ```
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-timer-wheel>

use std::collections::HashMap;

pub mod error;
pub use error::TimerError;

/// Ceiling on a timer's rounds counter. Held at `i32::MAX` rather than
/// `u32::MAX` so the Java port, whose counter is a signed `int`, refuses
/// exactly the same delays this one does.
const MAX_ROUNDS: u64 = i32::MAX as u64;

pub struct TimerWheel<V> {
    slots: Vec<Slot<V>>,
    mask: usize,
    hand: usize,
    next_id: u64,
    /// Live timers only: an id is removed here the moment it is cancelled or
    /// fired, so `pending()` never counts a timer the caller has retired.
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

    /// Largest delay the wheel can represent: a timer can sit out at most
    /// `i32::MAX` revolutions of `N` slots. Held at the signed bound rather
    /// than `u32::MAX` so the Java port refuses exactly the same delays.
    pub fn max_delay(&self) -> u64 {
        self.slots.len() as u64 * MAX_ROUNDS
    }

    /// Number of live (scheduled, not yet fired or cancelled) timers. A
    /// correct wheel returns this to 0 once every scheduled timer has fired;
    /// a leak would let it climb without bound.
    pub fn pending(&self) -> usize {
        self.id_to_slot.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id_to_slot.is_empty()
    }

    /// Entries physically held in one bucket, including cancelled ones not
    /// yet swept. Reading the spread across buckets is how you catch a
    /// workload whose delays all collide on one slot.
    pub fn slot_len(&self, slot: usize) -> usize {
        self.slots.get(slot).map_or(0, |s| s.entries.len())
    }

    /// Schedule `value` to fire in `delay_ticks`. Returns an id for cancel.
    ///
    /// A delay of 0 fires on the next tick, matching Netty's treatment of a
    /// deadline already in the past. A delay past [`Self::max_delay`] is
    /// clamped; use [`Self::try_schedule`] to have it refused instead.
    pub fn schedule(&mut self, delay_ticks: usize, value: V) -> u64 {
        let d = self.clamp_delay(delay_ticks);
        let id = self.next_id;
        self.next_id += 1;
        self.insert(id, d, value);
        id
    }

    /// Schedule `value`, refusing a delay the wheel cannot represent.
    pub fn try_schedule(&mut self, delay_ticks: usize, value: V) -> Result<u64, TimerError> {
        let max = self.max_delay();
        if delay_ticks as u64 > max {
            return Err(TimerError::DelayTooLong {
                delay: delay_ticks as u64,
                max,
            });
        }
        Ok(self.schedule(delay_ticks, value))
    }

    /// Mark a scheduled timer cancelled. Returns `true` if it was pending.
    pub fn cancel(&mut self, id: u64) -> bool {
        let Some(slot) = self.id_to_slot.remove(&id) else {
            return false;
        };
        // The entry keeps its seat until the hand reaches this bucket; only
        // the index is updated eagerly, which is what keeps `pending()` exact.
        for e in &mut self.slots[slot].entries {
            if e.id == id && !e.cancelled {
                e.cancelled = true;
                return true;
            }
        }
        false
    }

    /// Move a pending timer to a new delay, keeping its id. Returns `false`
    /// if the id is not pending (already fired, already cancelled, unknown).
    ///
    /// Unlike cancel this removes the entry eagerly - leaving a flagged
    /// entry behind would let one id sit in two buckets at once.
    pub fn reschedule(&mut self, id: u64, delay_ticks: usize) -> bool {
        let Some(slot) = self.id_to_slot.remove(&id) else {
            return false;
        };
        let Some(pos) = self.slots[slot]
            .entries
            .iter()
            .position(|e| e.id == id && !e.cancelled)
        else {
            return false;
        };
        let entry = self.slots[slot].entries.swap_remove(pos);
        let d = self.clamp_delay(delay_ticks);
        self.insert(id, d, entry.value);
        true
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

    /// Advance `ticks` ticks and return everything that fired across them,
    /// in tick order. A ticker thread that woke late catches up here rather
    /// than firing a whole revolution's timers on one bucket.
    pub fn advance(&mut self, ticks: usize) -> Vec<V> {
        let mut fired = Vec::new();
        for _ in 0..ticks {
            fired.append(&mut self.tick());
        }
        fired
    }

    /// Remove every pending timer and return its value. The hand stays where
    /// it is. This is the shutdown path: Netty's `HashedWheelTimer::stop`
    /// hands back the timeouts it never got to run, and so does this.
    pub fn drain(&mut self) -> Vec<V> {
        let mut out = Vec::with_capacity(self.id_to_slot.len());
        for slot in &mut self.slots {
            for e in std::mem::take(&mut slot.entries) {
                if !e.cancelled {
                    out.push(e.value);
                }
            }
        }
        self.id_to_slot.clear();
        out
    }

    /// Drop every pending timer and reset the hand. Ids already handed out
    /// are never reused, so a late `cancel` on a cleared timer returns
    /// `false` rather than hitting an unrelated timer.
    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.entries.clear();
        }
        self.id_to_slot.clear();
        self.hand = 0;
    }

    fn clamp_delay(&self, delay_ticks: usize) -> usize {
        let max = self.max_delay().min(usize::MAX as u64) as usize;
        delay_ticks.clamp(1, max)
    }

    fn insert(&mut self, id: u64, delay: usize, value: V) {
        let n = self.slots.len();
        let slot = self.hand.wrapping_add(delay) & self.mask;
        // rounds = ceil(d/N) - 1, not floor(d/N). They agree everywhere except
        // when d is an exact multiple of N, where the timer lands back on the
        // bucket the hand has just left and waits a full revolution for the
        // revisit. Charging a rounds counter for that revolution as well fires
        // the timer a lap late.
        let rounds = (delay.div_ceil(n) - 1) as u32;
        self.slots[slot].entries.push(Entry {
            id,
            rounds,
            value,
            cancelled: false,
        });
        self.id_to_slot.insert(id, slot);
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
