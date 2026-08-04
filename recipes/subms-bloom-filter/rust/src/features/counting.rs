//! Counting bloom filter: supports `remove()` by storing 4-bit
//! counters per cell instead of 1-bit flags. Cost: 4x memory vs the
//! base filter; gain: a real `remove()` operation that the base can't
//! support (since clearing a base bit can disturb other keys).
//!
//! Sized for ~1% FPR at 10 bits per key, k=7 (same defaults as the
//! base `BloomFilter`). Counter saturates at 15 to bound memory; on
//! saturation a `remove()` won't reduce the counter for that cell
//! (false-positive risk shifts slightly but no false negatives).

use crate::{FNV_OFFSET, FNV_PRIME, fnv1a64};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CountingBloomFilter {
    bit_count: u32,
    k: u32,
    /// 4 bits per cell - two cells per byte. Counter saturates at 15.
    cells: Vec<u8>,
}

impl CountingBloomFilter {
    /// Build an empty counting filter sized for `expected_entries`.
    pub fn new(expected_entries: usize) -> Self {
        let bit_count = expected_entries.saturating_mul(10).max(64) as u32;
        // 4 bits per cell -> bytes = (bit_count + 1) / 2 (round up).
        let bytes = (bit_count as usize).div_ceil(2);
        Self {
            bit_count,
            k: 7,
            cells: vec![0u8; bytes],
        }
    }

    pub fn bit_count(&self) -> u32 {
        self.bit_count
    }
    pub fn k(&self) -> u32 {
        self.k
    }

    /// Add a key. Increments the per-cell 4-bit counter at each of the
    /// `k` positions, saturating at 15.
    pub fn add(&mut self, key: &str) {
        let (h1, h2) = self.hash_pair(key);
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            self.incr(idx);
        }
    }

    /// Probabilistic membership query. No false negatives - if the
    /// key was added (and never removed enough to clear all `k`
    /// counters), this returns `true`. False positives still occur at
    /// the configured rate.
    pub fn might_contain(&self, key: &str) -> bool {
        let (h1, h2) = self.hash_pair(key);
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            if self.read(idx) == 0 {
                return false;
            }
        }
        true
    }

    /// Remove a key. Decrements each of the `k` counters. Cells that
    /// were saturated (counter == 15) stay at 15 - they cannot be
    /// decremented without risking false negatives for OTHER keys
    /// that incremented them past the saturation point.
    pub fn remove(&mut self, key: &str) {
        let (h1, h2) = self.hash_pair(key);
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            self.decr(idx);
        }
    }

    /// Zero every counter, keeping the allocation.
    pub fn clear(&mut self) {
        self.cells.fill(0);
    }

    fn hash_pair(&self, key: &str) -> (u32, u32) {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        (h1, h2)
    }

    fn read(&self, idx: u32) -> u8 {
        let byte = self.cells[(idx / 2) as usize];
        if idx % 2 == 0 {
            byte & 0x0f
        } else {
            (byte >> 4) & 0x0f
        }
    }

    fn write_cell(&mut self, idx: u32, value: u8) {
        let i = (idx / 2) as usize;
        let v = value & 0x0f;
        if idx % 2 == 0 {
            self.cells[i] = (self.cells[i] & 0xf0) | v;
        } else {
            self.cells[i] = (self.cells[i] & 0x0f) | (v << 4);
        }
    }

    fn incr(&mut self, idx: u32) {
        let cur = self.read(idx);
        if cur < 15 {
            self.write_cell(idx, cur + 1);
        }
    }

    fn decr(&mut self, idx: u32) {
        let cur = self.read(idx);
        // Don't decrement saturated cells - they may have been bumped
        // past 15 by other keys; we can't tell, so we hold them.
        if cur > 0 && cur < 15 {
            self.write_cell(idx, cur - 1);
        }
    }
}

// Keep the imports used by the file (silences unused-import warnings
// in builds where only one of the related items is referenced).
const _: u64 = FNV_PRIME;
const _: u64 = FNV_OFFSET;

#[cfg(test)]
#[path = "counting_tests.rs"]
mod tests;
