//! Compressed-bucket cuckoo filter. Stores each bucket as a
//! variable-length run of sorted fingerprints with a 1-byte count
//! prefix, instead of the fixed 4-byte slot array of the base filter.
//!
//! Memory shape per bucket:
//!
//! ```text
//! count : u8      ; 0..=BUCKET_SIZE
//! fps   : u8 * count   (sorted ascending)
//! ```
//!
//! At 50% load (2 fps/bucket average) the on-disk footprint is ~3
//! bytes/bucket vs 4 for the base; at 95% load it's ~5 vs 4 (we lose
//! to the base when buckets stay near-full). The win is the
//! low-to-moderate load regime, which is where most production cuckoo
//! filters sit (95% is the saturation cliff, not the operating point).
//!
//! Lookup is a binary search over a sorted run of at most BUCKET_SIZE
//! bytes; insert and delete keep the run sorted via memmove. Both pay
//! O(BUCKET_SIZE) per op vs O(BUCKET_SIZE) in the base, but with a
//! constant factor of ~2 for the move.

use crate::{BUCKET_SIZE, alt_index_of_fp, fnv1a64, mix};

const MAX_KICKS: usize = 500;

pub struct CompressedCuckooFilter {
    /// Flat backing buffer holding every bucket back-to-back. Per-bucket
    /// layout is `(count_byte, fp0, fp1, ...)` with `count_byte` slots
    /// of capacity. We pre-allocate the worst-case width per bucket so
    /// inserts don't need to shift the entire tail.
    buckets: Vec<[u8; BUCKET_SIZE + 1]>,
    mask: usize,
    count: usize,
    rng_state: u64,
}

impl CompressedCuckooFilter {
    pub fn with_capacity(expected_entries: usize) -> Self {
        let needed = (expected_entries.max(1) * 105 / 100) / BUCKET_SIZE + 1;
        let num_buckets = needed.max(2).next_power_of_two();
        Self {
            buckets: vec![[0u8; BUCKET_SIZE + 1]; num_buckets],
            mask: num_buckets - 1,
            count: 0,
            rng_state: 0x9e3779b97f4a7c15,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Byte cost of the live (occupied) state. Excludes the
    /// pre-allocated padding; this is the wire-format size.
    pub fn occupied_bytes(&self) -> usize {
        self.buckets.iter().map(|b| 1 + b[0] as usize).sum()
    }

    pub fn insert(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        if self.try_place(i1, fp) || self.try_place(i2, fp) {
            self.count += 1;
            return true;
        }
        let mut bucket_idx = if self.rand_bit() { i1 } else { i2 };
        let mut victim = fp;
        for _ in 0..MAX_KICKS {
            let slot = (self.next_random() as usize) % BUCKET_SIZE;
            victim = self.swap_at(bucket_idx, slot, victim);
            bucket_idx ^= alt_index_of_fp(victim) & self.mask;
            if self.try_place(bucket_idx, victim) {
                self.count += 1;
                return true;
            }
        }
        false
    }

    pub fn contains(&self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        self.bucket_has(i1, fp) || self.bucket_has(i2, fp)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        if self.bucket_remove(i1, fp) || self.bucket_remove(i2, fp) {
            self.count -= 1;
            return true;
        }
        false
    }

    fn indices(&self, key: &str) -> (u8, usize, usize) {
        let h = mix(fnv1a64(key.as_bytes()));
        let fp = ((h & 0xff) as u8).max(1);
        let i1 = (h >> 8) as usize & self.mask;
        let i2 = (i1 ^ alt_index_of_fp(fp)) & self.mask;
        (fp, i1, i2)
    }

    fn try_place(&mut self, i: usize, fp: u8) -> bool {
        let count = self.buckets[i][0] as usize;
        if count >= BUCKET_SIZE {
            return false;
        }
        // Insert fp keeping the run sorted ascending. Linear scan over
        // at most BUCKET_SIZE bytes; no allocation.
        let mut pos = 0;
        while pos < count && self.buckets[i][1 + pos] < fp {
            pos += 1;
        }
        // Shift tail one byte to the right, then write fp at the gap.
        for k in (pos..count).rev() {
            self.buckets[i][1 + k + 1] = self.buckets[i][1 + k];
        }
        self.buckets[i][1 + pos] = fp;
        self.buckets[i][0] = (count + 1) as u8;
        true
    }

    fn bucket_has(&self, i: usize, fp: u8) -> bool {
        let count = self.buckets[i][0] as usize;
        // Linear scan beats binary search at BUCKET_SIZE=4 (branch
        // predictor + SIMD-friendly). Still O(B) which is what we want.
        for k in 0..count {
            let cur = self.buckets[i][1 + k];
            if cur == fp {
                return true;
            }
            if cur > fp {
                return false;
            }
        }
        false
    }

    fn bucket_remove(&mut self, i: usize, fp: u8) -> bool {
        let count = self.buckets[i][0] as usize;
        for k in 0..count {
            if self.buckets[i][1 + k] == fp {
                // Shift the rest one byte left.
                for j in k..count - 1 {
                    self.buckets[i][1 + j] = self.buckets[i][1 + j + 1];
                }
                self.buckets[i][1 + count - 1] = 0;
                self.buckets[i][0] = (count - 1) as u8;
                return true;
            }
        }
        false
    }

    /// Swap a fingerprint at slot index `slot` inside bucket `i` and
    /// return the evicted fingerprint. Maintains the sorted invariant
    /// after the swap by removing the old value and re-inserting `fp`.
    fn swap_at(&mut self, i: usize, slot: usize, fp: u8) -> u8 {
        let count = self.buckets[i][0] as usize;
        let s = slot.min(count.saturating_sub(1));
        let victim = self.buckets[i][1 + s];
        // Remove victim, then re-insert fp via the sorted-insert path
        // so the bucket stays well-formed.
        for j in s..count - 1 {
            self.buckets[i][1 + j] = self.buckets[i][1 + j + 1];
        }
        self.buckets[i][1 + count - 1] = 0;
        self.buckets[i][0] = (count - 1) as u8;
        let placed = self.try_place(i, fp);
        debug_assert!(placed, "swap_at removed a slot but couldn't reinsert");
        victim
    }

    fn rand_bit(&mut self) -> bool {
        self.next_random() & 1 != 0
    }

    fn next_random(&mut self) -> u64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_below_saturation() {
        let mut cf = CompressedCuckooFilter::with_capacity(1000);
        for i in 0..500u32 {
            assert!(cf.insert(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(cf.contains(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(cf.delete(&format!("k{i}")));
        }
        assert_eq!(cf.len(), 0);
    }

    #[test]
    fn empty_filter_rejects_everything() {
        let cf = CompressedCuckooFilter::with_capacity(100);
        assert!(!cf.contains("never-inserted"));
        assert!(cf.is_empty());
        assert_eq!(cf.len(), 0);
    }

    #[test]
    fn occupied_bytes_grows_with_inserts() {
        let mut cf = CompressedCuckooFilter::with_capacity(500);
        let baseline = cf.occupied_bytes();
        for i in 0..200u32 {
            cf.insert(&format!("k{i}"));
        }
        assert!(cf.occupied_bytes() > baseline, "expected occupancy to grow");
    }

    #[test]
    fn delete_unknown_returns_false() {
        let mut cf = CompressedCuckooFilter::with_capacity(100);
        cf.insert("known");
        assert!(!cf.delete("never-inserted"));
        assert!(cf.contains("known"));
    }

    #[test]
    fn sorted_invariant_holds_through_inserts_and_deletes() {
        // Whitebox: after mixed ops every bucket's run must be sorted
        // ascending.
        let mut cf = CompressedCuckooFilter::with_capacity(500);
        for i in 0..400u32 {
            cf.insert(&format!("k{i}"));
        }
        for i in 0..200u32 {
            cf.delete(&format!("k{i}"));
        }
        for i in 400..500u32 {
            cf.insert(&format!("k{i}"));
        }
        for bucket in &cf.buckets {
            let count = bucket[0] as usize;
            for k in 1..count {
                assert!(
                    bucket[1 + k - 1] <= bucket[1 + k],
                    "bucket out of order: {bucket:?}"
                );
            }
        }
    }

    #[test]
    fn false_positive_rate_in_three_percent_range() {
        let n = 5_000usize;
        let mut cf = CompressedCuckooFilter::with_capacity(n);
        for i in 0..n {
            cf.insert(&format!("present{i}"));
        }
        let probes = 10_000usize;
        let mut fp = 0usize;
        for i in 0..probes {
            if cf.contains(&format!("absent{i}")) {
                fp += 1;
            }
        }
        let fpr = fp as f64 / probes as f64;
        assert!(fpr < 0.03, "fpr {fpr:.4} too high");
    }

    #[test]
    fn bucket_count_is_power_of_two() {
        let cf = CompressedCuckooFilter::with_capacity(1000);
        assert!(cf.bucket_count().is_power_of_two());
    }

    #[test]
    fn duplicate_inserts_stack_in_bucket() {
        let mut cf = CompressedCuckooFilter::with_capacity(100);
        cf.insert("dup");
        cf.insert("dup");
        cf.insert("dup");
        assert_eq!(cf.len(), 3);
        assert!(cf.contains("dup"));
        cf.delete("dup");
        cf.delete("dup");
        cf.delete("dup");
        assert!(!cf.contains("dup"));
    }
}
