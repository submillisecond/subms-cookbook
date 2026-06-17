//! Runnable example for the `subms-otel` recipe. Demonstrates the headline
//! pattern: register one [`SubMsObserver`] against a [`SubMsPerfHarness`] and
//! every recorded sample (and the post-bench summary) lands in OpenTelemetry,
//! without the workload itself ever pulling in the OTel dep tree.
//!
//! The library exposes:
//!
//! - [`TinyMap`] - a small open-addressed `u32 -> u32` map. Trivial on
//!   purpose; the point of the example is the observer wiring, not a
//!   state-of-the-art data structure.
//! - [`run_workload`] - builds a [`SubMsPerfHarness`] with the standard
//!   `subms.recipe.slug` / `subms.recipe.category` meta keys, exercises
//!   `put` / `get_hit` / `get_miss` stages annotated [`SubMsStageKind::HotPath`],
//!   and returns the populated harness so the caller can attach observers
//!   ahead of time.
//!
//! The driver in `examples/otel_main.rs` wires the sync [`OtelObserver`] and
//! the async [`OtelObserverAsync`] against this workload and dumps the
//! captured signal so the demo is self-contained (no Jaeger / Prometheus
//! / OTLP collector required).

use subms::{SubMsPerfHarness, SubMsStageKind};

pub use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver, SubMsStageSummary, summarize};
pub use subms_otel::{HISTOGRAM_NAME, HISTOGRAM_UNIT, OtelObserver, OtelObserverAsync};

/// Canonical slug surfaced via `meta["subms.recipe.slug"]`. Exposed so tests
/// can assert the attribute round-trips into OTel untouched.
pub const RECIPE_SLUG: &str = "subms-otel";

/// Canonical category surfaced via `meta["subms.recipe.category"]`. Downstream
/// dashboards filter on this attribute, so the example sets it to the
/// recipe's actual category.
pub const RECIPE_CATEGORY: &str = "adapter";

/// Tiny open-addressed `u32 -> u32` map. Linear probing; no resizing; load
/// factor is the caller's problem. Exists to give the harness something to
/// time without bringing in `HashMap`'s hashing cost.
///
/// A real recipe would carry tests, a quality bar, and a published
/// artefact; this is a stand-in whose only job is to produce a handful of
/// timed `put` / `get_hit` / `get_miss` samples.
pub struct TinyMap {
    slots: Vec<Option<(u32, u32)>>,
    mask: usize,
    len: usize,
}

impl TinyMap {
    /// Build with capacity rounded up to the next power of two. Capacity is
    /// in slots, not entries; aim for ~2x the working set.
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        Self {
            slots: vec![None; cap],
            mask: cap - 1,
            len: 0,
        }
    }

    /// Insert or update. Returns the previous value if the key was present.
    pub fn insert(&mut self, key: u32, value: u32) -> Option<u32> {
        let mut idx = hash_index(key, self.mask);
        loop {
            match self.slots[idx] {
                Some((k, v)) if k == key => {
                    self.slots[idx] = Some((key, value));
                    return Some(v);
                }
                Some(_) => idx = (idx + 1) & self.mask,
                None => {
                    self.slots[idx] = Some((key, value));
                    self.len += 1;
                    return None;
                }
            }
        }
    }

    /// Lookup. Returns `None` if the key is absent.
    pub fn get(&self, key: u32) -> Option<u32> {
        let mut idx = hash_index(key, self.mask);
        loop {
            match self.slots[idx] {
                Some((k, v)) if k == key => return Some(v),
                Some(_) => idx = (idx + 1) & self.mask,
                None => return None,
            }
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
}

fn hash_index(key: u32, mask: usize) -> usize {
    // splitmix32 - one round is enough for a toy map.
    let mut x = key.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    (x as usize) & mask
}

/// Parameters for [`run_workload`]. Kept explicit so the example and tests
/// can declare matching shapes without magic numbers.
#[derive(Clone, Copy, Debug)]
pub struct WorkloadParams {
    pub entries: u32,
    pub capacity: usize,
    pub seed: u64,
}

impl WorkloadParams {
    /// Default shape used by the example: 5_000 puts, 2x capacity, fixed
    /// seed so successive runs produce comparable OTEL output.
    pub fn small() -> Self {
        Self {
            entries: 5_000,
            capacity: 16_384,
            seed: 0x50BE_07E1_BADC_0FFE,
        }
    }
}

/// Build a [`SubMsPerfHarness`] annotated with the recipe-style meta keys
/// downstream OTEL filters expect, drive a [`TinyMap`] through `put`,
/// `get_hit`, and `get_miss` stages (each tagged [`SubMsStageKind::HotPath`]),
/// and return the populated harness.
///
/// The caller is expected to have attached an observer via
/// [`SubMsPerfHarness::with_observer`] before this is called - the workload
/// itself is observer-agnostic. That's the whole point of the example.
pub fn run_workload(mut h: SubMsPerfHarness, params: WorkloadParams) -> SubMsPerfHarness {
    h.input("entries", &params.entries.to_string());
    h.input("seed", &format!("{:#x}", params.seed));
    h.add_meta("subms.recipe.slug", RECIPE_SLUG);
    h.add_meta("subms.recipe.category", RECIPE_CATEGORY);
    h.add_meta("host", "example-local");
    h.add_meta("hardware_tier", "laptop");
    h.add_meta("crate_version", env!("CARGO_PKG_VERSION"));

    let mut map = TinyMap::with_capacity(params.capacity);
    let mut rng = SmallRng::new(params.seed);

    let put = h.stage("put", params.entries as usize);
    put.with_kind(SubMsStageKind::HotPath);
    for _ in 0..params.entries {
        let k = rng.next_u32();
        let v = rng.next_u32();
        put.time(|| {
            map.insert(k, v);
        });
    }

    let mut hit_rng = SmallRng::new(params.seed);
    let get_hit = h.stage("get_hit", params.entries as usize);
    get_hit.with_kind(SubMsStageKind::HotPath);
    for _ in 0..params.entries {
        let k = hit_rng.next_u32();
        let _drop = hit_rng.next_u32();
        get_hit.time(|| {
            // Read into a black-box-ish sink so the optimiser can't elide.
            let _ = std::hint::black_box(map.get(k));
        });
    }

    let mut miss_rng = SmallRng::new(params.seed ^ 0xDEAD_BEEF_DEAD_BEEF);
    let get_miss = h.stage("get_miss", params.entries as usize);
    get_miss.with_kind(SubMsStageKind::HotPath);
    for _ in 0..params.entries {
        // Flip the top bit so misses don't accidentally collide with the
        // populated key space.
        let k = miss_rng.next_u32() | 0x8000_0000;
        get_miss.time(|| {
            let _ = std::hint::black_box(map.get(k));
        });
    }

    h
}

/// Convenience: build a fresh harness, hand it the standard workload, return
/// it populated. The caller chains [`SubMsPerfHarness::with_observer`] before
/// or wires it after via [`SubMsPerfHarness::set_observer`].
pub fn standard_harness(params: WorkloadParams) -> SubMsPerfHarness {
    let h = SubMsPerfHarness::new("subms-otel-example", "rust");
    run_workload(h, params)
}

// --- internal -------------------------------------------------------------

/// Tiny splitmix64-style RNG. Same family as `SubMsLcg`; kept inline so the
/// example stays a two-dep package (subms + subms-otel) and the workload
/// doesn't depend on harness internals.
struct SmallRng {
    state: u64,
}

impl SmallRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
}
