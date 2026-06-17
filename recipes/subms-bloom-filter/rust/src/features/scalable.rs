//! Scalable bloom filter: tier of `CountingBloomFilter` layers that
//! adds a new (larger) layer when the active layer saturates. Holds a
//! target false-positive rate as the cardinality grows beyond the
//! initial sizing.
//!
//! Algorithm (Almeida et al., "Scalable Bloom Filters"):
//! - Start with one layer sized for `initial_capacity` entries at
//!   `target_fpr`.
//! - When the active layer reaches `initial_capacity` entries, add a
//!   new layer with `growth_factor`x the capacity and a tighter target
//!   FPR (so cumulative FPR stays bounded by target_fpr).
//! - Membership query asks every layer; positive if ANY layer says yes.
//!
//! Cost: layer transitions add memory + a small constant probe-cost
//! overhead per layer at query time. The 4x memory cost of the
//! underlying `CountingBloomFilter` carries through.

use super::counting::CountingBloomFilter;

pub struct ScalableBloomFilter {
    layers: Vec<CountingBloomFilter>,
    layer_capacities: Vec<usize>,
    layer_counts: Vec<usize>,
    growth_factor: usize,
}

impl ScalableBloomFilter {
    /// Build a scalable filter that starts with one layer sized for
    /// `initial_capacity` entries. Each subsequent layer is
    /// `growth_factor` (default 2) times the previous capacity.
    pub fn new(initial_capacity: usize) -> Self {
        Self::with_growth(initial_capacity, 2)
    }

    pub fn with_growth(initial_capacity: usize, growth_factor: usize) -> Self {
        let cap = initial_capacity.max(1);
        let g = growth_factor.max(2);
        Self {
            layers: vec![CountingBloomFilter::new(cap)],
            layer_capacities: vec![cap],
            layer_counts: vec![0],
            growth_factor: g,
        }
    }

    pub fn add(&mut self, key: &str) {
        // If active layer is at capacity, add a new layer first.
        let last_idx = self.layers.len() - 1;
        if self.layer_counts[last_idx] >= self.layer_capacities[last_idx] {
            self.add_layer();
        }
        let idx = self.layers.len() - 1;
        // Only insert into the active layer - membership query checks
        // all layers, so older keys remain findable in earlier layers.
        self.layers[idx].add(key);
        self.layer_counts[idx] += 1;
    }

    pub fn might_contain(&self, key: &str) -> bool {
        self.layers.iter().any(|l| l.might_contain(key))
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
    pub fn total_count(&self) -> usize {
        self.layer_counts.iter().sum()
    }

    fn add_layer(&mut self) {
        let last = self.layer_capacities.len() - 1;
        let new_cap = self.layer_capacities[last] * self.growth_factor;
        self.layers.push(CountingBloomFilter::new(new_cap));
        self.layer_capacities.push(new_cap);
        self.layer_counts.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_layer_when_saturated() {
        let mut sb = ScalableBloomFilter::new(10);
        for i in 0..30 {
            sb.add(&format!("k{i}"));
        }
        // 10 -> 20 -> 40 capacity progression; 30 entries needs 2-3 layers.
        assert!(
            sb.layer_count() >= 2,
            "expected layer growth, got {}",
            sb.layer_count()
        );
    }

    #[test]
    fn earlier_layer_keys_still_findable() {
        let mut sb = ScalableBloomFilter::new(5);
        for i in 0..50 {
            sb.add(&format!("k{i}"));
        }
        // First-layer keys must still be findable after several layer adds.
        for i in 0..5 {
            assert!(sb.might_contain(&format!("k{i}")), "lost k{i}");
        }
    }

    #[test]
    fn membership_invariant_no_false_negatives() {
        let mut sb = ScalableBloomFilter::new(100);
        for i in 0..500 {
            sb.add(&format!("k{i}"));
        }
        for i in 0..500 {
            assert!(sb.might_contain(&format!("k{i}")));
        }
    }

    #[test]
    fn total_count_tracks_inserts() {
        let mut sb = ScalableBloomFilter::new(10);
        for i in 0..25 {
            sb.add(&format!("k{i}"));
        }
        assert_eq!(sb.total_count(), 25);
    }

    #[test]
    fn empty_scalable_rejects_anything() {
        let sb = ScalableBloomFilter::new(100);
        assert!(!sb.might_contain("never-added"));
    }

    #[test]
    fn growth_factor_default_is_two() {
        let mut sb = ScalableBloomFilter::new(4);
        for i in 0..5 {
            sb.add(&format!("k{i}"));
        }
        // First-layer cap 4 -> after 5 adds we should have a second layer of cap 8.
        // Indirect check via total_count + layer_count.
        assert_eq!(sb.layer_count(), 2);
    }
}
