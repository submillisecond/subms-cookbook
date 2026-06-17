//! Dynamic cuckoo filter: chains a fresh cuckoo filter at each load
//! milestone so the structure grows past its initial sizing without
//! the rejection-at-saturation behaviour of the base filter.
//!
//! Algorithm (Chen et al., "Dynamic Cuckoo Filter", 2017): when the
//! active filter's load factor passes `grow_threshold` (default 0.95),
//! allocate a new filter at double the bucket count and start inserting
//! there. Membership query asks every filter; positive if ANY layer
//! says yes. Delete probes every layer in newest-first order and
//! removes from the first match (deleting from older layers preserves
//! the structural integrity of newer ones).
//!
//! Why a chain instead of migration: cuckoo filters store partial-key
//! fingerprints, not keys. Re-bucketing a fingerprint after capacity
//! grows can lose track of the second candidate bucket (the alt-index
//! depends on the fingerprint, not the key). The DCF paper sidesteps
//! this by chaining filters, which gives O(1) amortised insert and
//! O(L) query where L is the chain length.

use crate::CuckooFilter;

const DEFAULT_INITIAL_BUCKETS_HINT: usize = 1024;
const DEFAULT_GROW_THRESHOLD: f64 = 0.95;
const GROWTH_FACTOR: usize = 2;
/// Slots per bucket. Matches the base filter; layers always share the
/// same bucket size so query semantics stay consistent.
const BUCKET_SIZE: usize = 4;

pub struct DynamicCuckooFilter {
    layers: Vec<CuckooFilter>,
    layer_capacities: Vec<usize>,
    grow_threshold: f64,
}

impl DynamicCuckooFilter {
    /// Build a dynamic cuckoo filter starting sized for
    /// `initial_capacity` entries. Auto-grows at 95% load.
    pub fn new(initial_capacity: usize) -> Self {
        Self::with_threshold(initial_capacity, DEFAULT_GROW_THRESHOLD)
    }

    /// Build with a custom grow threshold in `(0.0, 1.0)`. Lower
    /// thresholds grow earlier (more layers, lower per-layer pressure);
    /// higher thresholds delay growth at the cost of risk-of-rejection.
    pub fn with_threshold(initial_capacity: usize, grow_threshold: f64) -> Self {
        let cap = initial_capacity.max(DEFAULT_INITIAL_BUCKETS_HINT / BUCKET_SIZE);
        let t = if grow_threshold.is_finite() && grow_threshold > 0.0 && grow_threshold < 1.0 {
            grow_threshold
        } else {
            DEFAULT_GROW_THRESHOLD
        };
        Self {
            layers: vec![CuckooFilter::with_capacity(cap)],
            layer_capacities: vec![cap],
            grow_threshold: t,
        }
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn len(&self) -> usize {
        self.layers.iter().map(|l| l.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Insert a key into the active layer. Grows if the active layer
    /// crosses the load threshold OR rejects the insert outright.
    pub fn insert(&mut self, key: &str) -> bool {
        if self.should_grow() {
            self.grow();
        }
        let active = self.layers.len() - 1;
        if self.layers[active].insert(key) {
            return true;
        }
        // Saturation at the active layer despite the threshold check:
        // grow once more and retry. Layer growth is bounded by the
        // global `len()` so this terminates even under pathological
        // collisions.
        self.grow();
        let active = self.layers.len() - 1;
        self.layers[active].insert(key)
    }

    /// Membership over every layer. False positives possible (same FPR
    /// model as the base; cumulative FPR is bounded by sum across
    /// layers but typically dominated by the active layer).
    pub fn contains(&self, key: &str) -> bool {
        self.layers.iter().any(|l| l.contains(key))
    }

    /// Delete a single occurrence. Probes newest-first so duplicate
    /// keys are removed in reverse insertion order.
    pub fn delete(&mut self, key: &str) -> bool {
        for layer in self.layers.iter_mut().rev() {
            if layer.delete(key) {
                return true;
            }
        }
        false
    }

    pub fn load_factor(&self) -> f64 {
        let active = self.layers.len() - 1;
        let cap = self.layer_capacities[active];
        if cap == 0 {
            0.0
        } else {
            self.layers[active].len() as f64 / cap as f64
        }
    }

    fn should_grow(&self) -> bool {
        let active = self.layers.len() - 1;
        let cap = self.layer_capacities[active];
        if cap == 0 {
            return false;
        }
        self.layers[active].len() as f64 / cap as f64 >= self.grow_threshold
    }

    fn grow(&mut self) {
        let last = self.layer_capacities.len() - 1;
        let new_cap = self.layer_capacities[last] * GROWTH_FACTOR;
        self.layers.push(CuckooFilter::with_capacity(new_cap));
        self.layer_capacities.push(new_cap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_below_threshold() {
        let mut d = DynamicCuckooFilter::new(2000);
        for i in 0..500u32 {
            assert!(d.insert(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(d.contains(&format!("k{i}")));
        }
        assert_eq!(d.layer_count(), 1, "no grow expected below threshold");
    }

    #[test]
    fn grows_when_threshold_crossed() {
        // 256 cap -> threshold 0.95 ~ 243 entries triggers a grow.
        let mut d = DynamicCuckooFilter::with_threshold(256, 0.5);
        for i in 0..1000u32 {
            assert!(d.insert(&format!("k{i}")));
        }
        assert!(
            d.layer_count() >= 2,
            "expected growth, got {} layers",
            d.layer_count()
        );
        // Membership invariant: every inserted key is findable across layers.
        for i in 0..1000u32 {
            assert!(d.contains(&format!("k{i}")), "lost k{i}");
        }
    }

    #[test]
    fn delete_walks_layers_newest_first() {
        let mut d = DynamicCuckooFilter::with_threshold(64, 0.25);
        for i in 0..200u32 {
            d.insert(&format!("k{i}"));
        }
        assert!(d.layer_count() >= 2);
        // Every previously-inserted key deletes once.
        for i in 0..200u32 {
            assert!(d.delete(&format!("k{i}")), "could not delete k{i}");
        }
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn delete_unknown_returns_false() {
        let mut d = DynamicCuckooFilter::new(100);
        d.insert("known");
        assert!(!d.delete("never-inserted"));
        assert!(d.contains("known"));
    }

    #[test]
    fn empty_filter_rejects_anything() {
        let d = DynamicCuckooFilter::new(100);
        assert!(!d.contains("any"));
        assert!(d.is_empty());
    }

    #[test]
    fn load_factor_resets_after_grow() {
        let mut d = DynamicCuckooFilter::with_threshold(64, 0.4);
        for i in 0..200u32 {
            d.insert(&format!("k{i}"));
        }
        // After growing, the active (newest) layer is the relevant one
        // for load_factor; it should be far below 1.0.
        assert!(d.load_factor() < 1.0);
    }

    #[test]
    fn cumulative_len_tracks_all_layers() {
        let mut d = DynamicCuckooFilter::with_threshold(64, 0.3);
        let n = 500;
        for i in 0..n {
            d.insert(&format!("k{i}"));
        }
        assert_eq!(d.len(), n as usize);
        assert!(d.layer_count() >= 2);
    }

    #[test]
    fn invalid_threshold_falls_back_to_default() {
        // NaN, 0, 1+ all fall back to 0.95.
        let d = DynamicCuckooFilter::with_threshold(100, f64::NAN);
        assert!((d.grow_threshold - 0.95).abs() < 1e-12);
        let d2 = DynamicCuckooFilter::with_threshold(100, 1.5);
        assert!((d2.grow_threshold - 0.95).abs() < 1e-12);
    }
}
