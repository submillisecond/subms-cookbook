//! Partitioned bloom filter: `k` independent slices of `m/k` bits
//! each. Hash function `i` writes/reads only into slice `i`, so the
//! filter behaves like `k` parallel 1-hash filters AND'd together.
//!
//! Why bother? Two reasons:
//! 1. The independent-slice model gives cleaner FPR math: cumulative
//!    P(false positive) = `(1 - (1 - 1/(m/k))^n)^k`, vs the
//!    shared-array variant's looser bound.
//! 2. Each slice can be updated by an independent producer without
//!    coordination - a useful property for fan-in scenarios where N
//!    threads each own one hash function.
//!
//! Same default sizing as the base: ~10 bits/key, k=7.

use crate::fnv1a64;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PartitionedBloomFilter {
    /// Bits per slice (m/k).
    slice_bits: u32,
    k: u32,
    /// k slices, each `slice_bits` long, packed into u64 words.
    /// slice i lives at slices[i] (a Vec<u64>).
    slices: Vec<Vec<u64>>,
}

impl PartitionedBloomFilter {
    pub fn new(expected_entries: usize) -> Self {
        let bit_count = expected_entries.saturating_mul(10).max(64) as u32;
        let k = 7u32;
        let slice_bits = bit_count.div_ceil(k);
        let words = (slice_bits as usize).div_ceil(64);
        Self {
            slice_bits,
            k,
            slices: (0..k as usize).map(|_| vec![0u64; words]).collect(),
        }
    }

    pub fn slice_bits(&self) -> u32 {
        self.slice_bits
    }
    pub fn k(&self) -> u32 {
        self.k
    }
    pub fn bit_count(&self) -> u32 {
        self.slice_bits * self.k
    }

    /// Add a key. Each of the `k` hash positions writes into its own
    /// slice - i.e. hash i sets one bit in slice i, independently.
    pub fn add(&mut self, key: &str) {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.slice_bits;
            self.slices[i as usize][(idx / 64) as usize] |= 1u64 << (idx % 64);
        }
    }

    pub fn might_contain(&self, key: &str) -> bool {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.slice_bits;
            if self.slices[i as usize][(idx / 64) as usize] & (1u64 << (idx % 64)) == 0 {
                return false;
            }
        }
        true
    }

    /// Update only slice `i` for the given key. Lets a producer that
    /// owns hash `i` add a key without coordinating with producers
    /// that own other slices. Reader must observe all `k` slices.
    pub fn add_to_slice(&mut self, key: &str, slice: usize) {
        assert!(slice < self.k as usize, "slice index out of range");
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        let idx = h1.wrapping_add((slice as u32).wrapping_mul(h2)) % self.slice_bits;
        self.slices[slice][(idx / 64) as usize] |= 1u64 << (idx % 64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_keys_always_match() {
        let mut pb = PartitionedBloomFilter::new(1000);
        for i in 0..1000 {
            pb.add(&format!("k{i}"));
        }
        for i in 0..1000 {
            assert!(pb.might_contain(&format!("k{i}")));
        }
    }

    #[test]
    fn empty_filter_rejects_everything() {
        let pb = PartitionedBloomFilter::new(100);
        assert!(!pb.might_contain("any"));
    }

    #[test]
    fn fpr_target_holds_with_default_sizing() {
        let mut pb = PartitionedBloomFilter::new(10_000);
        for i in 0..10_000 {
            pb.add(&format!("present{i}"));
        }
        let probes = 100_000;
        let mut fp = 0;
        for i in 0..probes {
            if pb.might_contain(&format!("absent{i}")) {
                fp += 1;
            }
        }
        let fpr = fp as f64 / probes as f64;
        assert!(fpr < 0.05, "FPR {fpr:.4} exceeded 5%");
    }

    #[test]
    fn slice_count_equals_k() {
        let pb = PartitionedBloomFilter::new(100);
        assert_eq!(pb.slices.len(), pb.k() as usize);
    }

    #[test]
    fn add_to_slice_independent_of_full_add() {
        let mut pb = PartitionedBloomFilter::new(100);
        // Per-slice add for a key WITHOUT a full add: membership
        // should be NO because only one slice has the bit.
        pb.add_to_slice("partial", 0);
        assert!(!pb.might_contain("partial"));

        // Now do the per-slice add for all k slices manually: should
        // become a match.
        for i in 1..pb.k() as usize {
            pb.add_to_slice("partial", i);
        }
        assert!(pb.might_contain("partial"));
    }

    #[test]
    #[should_panic(expected = "slice index out of range")]
    fn add_to_slice_rejects_bad_index() {
        let mut pb = PartitionedBloomFilter::new(100);
        pb.add_to_slice("k", 999);
    }
}
