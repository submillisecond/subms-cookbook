//! Runnable companion to the `subms-perf-harness` primer. The point is to show
//! the harness end-to-end on a workload small enough to read in one sitting -
//! a tiny open-addressed `u32 -> u32` map measured at insert / hit-lookup /
//! miss-lookup. Nothing here is meant to compete with `std::collections` or
//! `hashbrown`; the map is the workload, the harness is the subject.
//!
//! The shape mirrors a published recipe:
//!
//! - [`TinyMap`] is the thing under test.
//! - [`HarnessRecipe`] implements [`subms::SubMsRecipe`], records three stages
//!   (`put`, `get_hit`, `get_miss`), and is what `examples/perf_main.rs` drives.
//! - [`SUB_MS_NS`] is the p99 budget every stage asserts against.
//!
//! Stage names follow the cookbook's k/v convention so the diff / drift
//! actions correlate this primer against the same naming reviewers already
//! recognise from real recipes.

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

/// Per-stage p99 ceiling in nanoseconds. One millisecond - the cookbook's
/// non-negotiable line. Exposed so the example, the test, and a downstream
/// reader can all reference the same number.
pub const SUB_MS_NS: u64 = 1_000_000;

/// A tiny open-addressed `u32 -> u32` map. Linear probing, power-of-two
/// capacity, 50% load factor before grow. No erase - the workload doesn't
/// need one, and skipping tombstones keeps the probe loop branch-light.
///
/// `u32` keys + `u32` values are deliberate. Boxed strings would bury the
/// harness signal under allocator noise; the primer is about measuring the
/// timing path, not about producing a state-of-the-art map.
pub struct TinyMap {
    slots: Vec<Slot>,
    mask: u32,
    len: usize,
}

#[derive(Clone, Copy)]
struct Slot {
    key: u32,
    value: u32,
    occupied: bool,
}

impl Slot {
    const EMPTY: Slot = Slot {
        key: 0,
        value: 0,
        occupied: false,
    };
}

impl TinyMap {
    /// Build a map sized for at least `expected` inserts at a 50% load factor.
    /// Capacity is rounded up to the next power of two and floored at 16, so
    /// the probe-loop mask stays cheap on small inputs.
    pub fn with_capacity(expected: usize) -> Self {
        let want = expected.saturating_mul(2).max(16);
        let cap = want.next_power_of_two();
        Self {
            slots: vec![Slot::EMPTY; cap],
            mask: (cap - 1) as u32,
            len: 0,
        }
    }

    /// Inserts or overwrites `(key, value)`. Returns the previous value, if any.
    pub fn put(&mut self, key: u32, value: u32) -> Option<u32> {
        if (self.len + 1) * 2 > self.slots.len() {
            self.grow();
        }
        let mut idx = (Self::mix(key) & self.mask) as usize;
        loop {
            let slot = &mut self.slots[idx];
            if !slot.occupied {
                *slot = Slot {
                    key,
                    value,
                    occupied: true,
                };
                self.len += 1;
                return None;
            }
            if slot.key == key {
                let prev = slot.value;
                slot.value = value;
                return Some(prev);
            }
            idx = (idx + 1) & (self.mask as usize);
        }
    }

    /// Lookup. Returns `Some(value)` on hit, `None` on miss.
    pub fn get(&self, key: u32) -> Option<u32> {
        let mut idx = (Self::mix(key) & self.mask) as usize;
        loop {
            let slot = &self.slots[idx];
            if !slot.occupied {
                return None;
            }
            if slot.key == key {
                return Some(slot.value);
            }
            idx = (idx + 1) & (self.mask as usize);
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    fn grow(&mut self) {
        let new_cap = self.slots.len().saturating_mul(2);
        let old = std::mem::replace(&mut self.slots, vec![Slot::EMPTY; new_cap]);
        self.mask = (self.slots.len() - 1) as u32;
        self.len = 0;
        for slot in old.into_iter().filter(|s| s.occupied) {
            self.put(slot.key, slot.value);
        }
    }

    // Splitmix-style mixer. `u32` keys go through one fold so dense sequential
    // inputs (the workload the primer drives) don't pile up in a single bucket.
    #[inline]
    fn mix(key: u32) -> u32 {
        let mut x = key.wrapping_mul(0x9E37_79B1);
        x ^= x >> 16;
        x = x.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 13;
        x
    }
}

/// `SubMsRecipe` wiring the TinyMap workload through the harness.
///
/// Stages, in registration order:
///
/// - `put` - `entries` insertions of distinct keys
/// - `get_hit` - `entries` lookups for keys that were just inserted
/// - `get_miss` - `entries` lookups for keys outside the inserted range
///
/// Both warmup passes prime the icache and the branch predictor before any
/// sample is recorded. On the AOT side it's mostly cache work; the matching
/// Java sibling needs the warmup harder because HotSpot wouldn't have reached
/// C2 otherwise. Both sides spell the same shape so a reader translating
/// between them isn't doing it without a map.
pub struct HarnessRecipe;

impl SubMsRecipe for HarnessRecipe {
    fn name(&self) -> &str {
        "tinymap"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;

        let mut map = TinyMap::with_capacity(entries);

        for i in 0..warmup {
            map.put(warm_key(i), i as u32);
        }
        // Throw away warm-up state so the timed insert stage starts clean.
        map = TinyMap::with_capacity(entries);

        {
            let stage = h.stage("put", entries);
            for i in 0..entries {
                let key = i as u32;
                let value = i as u32;
                stage.time(|| {
                    map.put(key, value);
                });
            }
        }

        {
            let stage = h.stage("get_hit", entries);
            let mut rng = SubMsLcg::new(seed);
            for _ in 0..entries {
                let key = rng.bounded(entries as u32);
                stage.time(|| {
                    let _ = map.get(key);
                });
            }
        }

        {
            let stage = h.stage("get_miss", entries);
            let mut rng = SubMsLcg::new(seed.wrapping_add(1));
            let miss_base = entries as u32;
            for _ in 0..entries {
                let key = miss_base + rng.bounded(entries as u32);
                stage.time(|| {
                    let _ = map.get(key);
                });
            }
        }

        h.add_meta("workload", "tinymap");
        h.add_meta("load_factor_max", "0.5");
        h.add_meta("hardware_tier", "laptop");
    }
}

/// The set of stage-level p99 ceilings the primer asserts on every run. Each
/// stage budgets the full sub-millisecond line so a reader copying this shape
/// into their own recipe starts on the same ground.
pub fn default_assertions() -> [subms::SubMsBenchAssertion; 3] {
    [
        subms::SubMsBenchAssertion {
            stage: "put",
            p99_ns_max: SUB_MS_NS,
        },
        subms::SubMsBenchAssertion {
            stage: "get_hit",
            p99_ns_max: SUB_MS_NS,
        },
        subms::SubMsBenchAssertion {
            stage: "get_miss",
            p99_ns_max: SUB_MS_NS,
        },
    ]
}

// Stable warm-up key derivation. Pulled out of the recipe body so tests can
// reach the same shape without dragging the whole harness in.
fn warm_key(i: usize) -> u32 {
    // Bias warm-up keys away from the timed [0, entries) range so the warm-up
    // doesn't pre-populate the map under test.
    (i as u32).wrapping_add(0xFFFF_0000)
}
