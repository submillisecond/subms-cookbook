//! Hierarchical timer wheel (HHW). Three levels, each a wheel of
//! 64 slots: seconds, minutes (each slot = 64 ticks), hours (each
//! slot = 64*64 ticks). A timer scheduled `d` ticks out lands on the
//! coarsest wheel whose slot can hold it; on each tick of a higher
//! wheel we cascade its expiring slot's entries down to the lower
//! wheel re-binned at the residual offset.
//!
//! Capacity: 64 * 64 * 64 = 262_144 ticks per "day" (loose analogy).
//! Long delays no longer cost a no-op revolution per `mask` ticks the
//! way the base wheel does; they sit on the coarse wheel and get
//! cascaded down only as their fire time approaches.
//!
//! Memory: 3 * 64 = 192 buckets total regardless of how many timers
//! are scheduled - the buckets hold Vecs of entries, not a per-tick
//! slot. Compare with the base single-level wheel which needs a slot
//! count >= max-delay for O(1) firing.

use crate::TimerError;

const LEVELS: usize = 3;
const SLOTS: usize = 64;
const MASK: usize = SLOTS - 1;
const LEVEL_SHIFT: [u32; LEVELS] = [0, 6, 12];
const LEVEL_RANGE: [usize; LEVELS] = [
    SLOTS,                 // level 0: 1..=64 ticks
    SLOTS * SLOTS,         // level 1: 65..=4096 ticks
    SLOTS * SLOTS * SLOTS, // level 2: 4097..=262_144 ticks
];

struct Entry<V> {
    id: u64,
    deadline: u64,
    value: Option<V>,
    cancelled: bool,
}

pub struct HierarchicalTimerWheel<V> {
    /// Three wheels of 64 buckets each.
    wheels: [[Vec<Entry<V>>; SLOTS]; LEVELS],
    /// Monotonically increasing tick counter. The level-i slot for
    /// `t` is `(t >> LEVEL_SHIFT[i]) & MASK`.
    now: u64,
    next_id: u64,
    /// Counts cascade events (entries moved from a coarser wheel down
    /// to a finer one). Useful for diagnostics; doubles as a stable
    /// hook for the metrics feature.
    cascades: u64,
    /// Live entries. Tracked rather than derived because counting means
    /// walking all 192 buckets, and callers poll this on every tick.
    pending: usize,
}

impl<V> HierarchicalTimerWheel<V> {
    pub fn new() -> Self {
        // const-init a 3x64 array of empty Vecs. The repeat-with shape
        // avoids requiring V: Clone.
        let wheels = std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
        Self {
            wheels,
            now: 0,
            next_id: 1,
            cascades: 0,
            pending: 0,
        }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn cascades(&self) -> u64 {
        self.cascades
    }

    /// Live (scheduled, not yet fired or cancelled) timers.
    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn is_empty(&self) -> bool {
        self.pending == 0
    }

    /// Max delay (in ticks) the wheel can place without overflowing the
    /// coarsest level. Schedules beyond this cap are rejected by
    /// [`Self::try_schedule`] and clamped by [`Self::schedule`].
    pub const fn max_delay() -> usize {
        LEVEL_RANGE[LEVELS - 1]
    }

    /// Schedule `value` to fire in `delay` ticks. Delays larger than
    /// [`Self::max_delay`] are clamped to the cap; use
    /// [`Self::try_schedule`] for explicit overflow handling.
    pub fn schedule(&mut self, delay: u64, value: V) -> u64 {
        let cap = Self::max_delay() as u64;
        let d = delay.min(cap.saturating_sub(1));
        self.try_schedule(d, value).expect("clamped delay fits")
    }

    pub fn try_schedule(&mut self, delay: u64, value: V) -> Result<u64, TimerError> {
        let max = Self::max_delay() as u64;
        if delay >= max {
            return Err(TimerError::DelayTooLong { delay, max });
        }
        let id = self.next_id;
        self.next_id += 1;
        self.insert(id, self.now + delay, value);
        Ok(id)
    }

    /// Mark `id` cancelled. Returns true if a pending entry was found.
    /// O(n) over every bucket; the tradeoff vs the base wheel (which
    /// keeps an id->slot map) is that the hierarchical wheel moves
    /// entries on cascade, so an id->slot map would need to be patched
    /// on every cascade. Linear sweep on cancel is the cheaper deal.
    pub fn cancel(&mut self, id: u64) -> bool {
        for lvl in 0..LEVELS {
            for slot in 0..SLOTS {
                for e in &mut self.wheels[lvl][slot] {
                    if e.id == id && !e.cancelled {
                        e.cancelled = true;
                        e.value = None;
                        self.pending -= 1;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Move a pending timer to a new delay, keeping its id. Pays the same
    /// linear sweep as [`Self::cancel`], for the same reason.
    pub fn reschedule(&mut self, id: u64, delay: u64) -> bool {
        let cap = Self::max_delay() as u64;
        let d = delay.min(cap.saturating_sub(1));
        for lvl in 0..LEVELS {
            for slot in 0..SLOTS {
                let Some(pos) = self.wheels[lvl][slot]
                    .iter()
                    .position(|e| e.id == id && !e.cancelled)
                else {
                    continue;
                };
                let entry = self.wheels[lvl][slot].swap_remove(pos);
                let Some(value) = entry.value else {
                    return false;
                };
                self.pending -= 1;
                self.insert(id, self.now + d, value);
                return true;
            }
        }
        false
    }

    /// Remove every pending timer and return its value; the tick counter
    /// stays where it is.
    pub fn drain(&mut self) -> Vec<V> {
        let mut out = Vec::with_capacity(self.pending);
        for lvl in 0..LEVELS {
            for slot in 0..SLOTS {
                for mut e in std::mem::take(&mut self.wheels[lvl][slot]) {
                    if e.cancelled {
                        continue;
                    }
                    if let Some(v) = e.value.take() {
                        out.push(v);
                    }
                }
            }
        }
        self.pending = 0;
        out
    }

    /// Drop every pending timer and reset the tick counter.
    pub fn clear(&mut self) {
        for lvl in 0..LEVELS {
            for slot in 0..SLOTS {
                self.wheels[lvl][slot].clear();
            }
        }
        self.pending = 0;
        self.now = 0;
    }

    fn insert(&mut self, id: u64, deadline: u64, value: V) {
        let entry = Entry {
            id,
            deadline,
            value: Some(value),
            cancelled: false,
        };
        let (lvl, slot) = self.bucket_for(deadline);
        self.wheels[lvl][slot].push(entry);
        self.pending += 1;
    }

    /// Advance one tick. Returns the values of all timers whose
    /// deadline equals the new `now`. Cascade from coarser wheels
    /// down to finer wheels as needed.
    pub fn tick(&mut self) -> Vec<V> {
        self.now += 1;
        // Cascade higher levels whose slot is about to roll over.
        // The slot index at level L wraps every `LEVEL_RANGE[L]`
        // ticks; when the lower-level index wraps to 0, the next
        // higher level's slot has new contents to push down.
        // Walk highest-to-lowest so a level-2 entry cascading down
        // to level 1 still has time to re-cascade to level 0 on the
        // same tick when its deadline is now.
        for lvl in (1..LEVELS).rev() {
            let lower_period = 1u64 << LEVEL_SHIFT[lvl];
            if self.now % lower_period == 0 {
                let slot = ((self.now >> LEVEL_SHIFT[lvl]) as usize) & MASK;
                // Move entries from wheels[lvl][slot] down to their
                // correct lower-level slot now that we're closer in time.
                let entries = std::mem::take(&mut self.wheels[lvl][slot]);
                for e in entries {
                    if e.cancelled {
                        continue;
                    }
                    self.cascades += 1;
                    let (new_lvl, new_slot) = self.bucket_for(e.deadline);
                    self.wheels[new_lvl][new_slot].push(e);
                }
            }
        }

        let slot = (self.now as usize) & MASK;
        let entries = std::mem::take(&mut self.wheels[0][slot]);
        let mut fired = Vec::new();
        for mut e in entries {
            if e.cancelled {
                continue;
            }
            if e.deadline != self.now {
                // An entry rebinned into this level-0 slot whose deadline is
                // still a revolution away. LEVEL_RANGE[0]=64 leaves no room
                // for it today; re-binning rather than dropping keeps the
                // wheel correct if the level spans are ever retuned.
                let (lvl, slot) = self.bucket_for(e.deadline);
                self.wheels[lvl][slot].push(e);
                continue;
            }
            self.pending -= 1;
            if let Some(v) = e.value.take() {
                fired.push(v);
            }
        }
        fired
    }

    /// Pick the coarsest level whose slot range contains `deadline -
    /// now`, then the slot within that level.
    fn bucket_for(&self, deadline: u64) -> (usize, usize) {
        let diff = deadline.saturating_sub(self.now);
        let lvl = if diff < LEVEL_RANGE[0] as u64 {
            0
        } else if diff < LEVEL_RANGE[1] as u64 {
            1
        } else {
            2
        };
        let slot = ((deadline >> LEVEL_SHIFT[lvl]) as usize) & MASK;
        (lvl, slot)
    }
}

impl<V> Default for HierarchicalTimerWheel<V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "hierarchical_tests.rs"]
mod tests;
